# Give a Windows executable Magnetite's icon and file properties.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File brand-exe.ps1 <exe> <ico> <version>
#
# scripts/build.sh uses it on the package's magnetite.exe. That file is CEF's
# bootstrap.exe: it would show CEF's icon in Explorer and the taskbar, and
# Windows would call it "CEF Bootstrap Application" (Task Manager, the list
# of default apps) after its file properties.
#
# - Icon: the icon groups the executable already has keep their names (the
#   program loads its window icon by name); their images are replaced by the
#   .ico file's.
# - Properties: the version resource is replaced by one that names Magnetite,
#   with <version> (as in Cargo.toml, e.g. 0.2.0).
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Ico,
    [Parameter(Mandatory = $true)][string]$Version
)
$ErrorActionPreference = 'Stop'

Add-Type @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public static class Brand {
    const int RT_ICON = 3, RT_GROUP_ICON = 14, RT_VERSION = 16;
    const uint LOAD_LIBRARY_AS_DATAFILE = 2;
    const ushort ENGLISH_US = 1033;

    delegate bool EnumNameProc(IntPtr module, IntPtr type, IntPtr name, IntPtr param);
    delegate bool EnumLangProc(IntPtr module, IntPtr type, IntPtr name, ushort lang, IntPtr param);

    [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern IntPtr LoadLibraryEx(string file, IntPtr reserved, uint flags);
    [DllImport("kernel32")]
    static extern bool FreeLibrary(IntPtr module);
    [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool EnumResourceNames(IntPtr module, IntPtr type, EnumNameProc callback, IntPtr param);
    [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern bool EnumResourceLanguages(IntPtr module, IntPtr type, IntPtr name, EnumLangProc callback, IntPtr param);
    [DllImport("kernel32", SetLastError = true, CharSet = CharSet.Unicode)]
    static extern IntPtr BeginUpdateResource(string file, bool deleteExisting);
    [DllImport("kernel32", SetLastError = true)]
    static extern bool UpdateResource(IntPtr update, IntPtr type, IntPtr name, ushort lang, byte[] data, uint size);
    [DllImport("kernel32", SetLastError = true)]
    static extern bool EndUpdateResource(IntPtr update, bool discard);

    // A resource name: a number, or a string (then `Text` is set).
    class Name {
        public ushort Id; public string Text; public ushort Lang;
        public IntPtr Pointer() {
            return Text == null ? (IntPtr)Id : Marshal.StringToHGlobalUni(Text);
        }
    }

    static List<Name> Existing(IntPtr module, int type) {
        var found = new List<Name>();
        EnumResourceNames(module, (IntPtr)type, (m, t, name, p) => {
            bool isId = ((ulong)name.ToInt64() >> 16) == 0;
            ushort id = isId ? (ushort)name.ToInt64() : (ushort)0;
            string text = isId ? null : Marshal.PtrToStringUni(name);
            EnumResourceLanguages(m, t, name, (m2, t2, n2, lang, p2) => {
                found.Add(new Name { Id = id, Text = text, Lang = lang });
                return true;
            }, IntPtr.Zero);
            return true;
        }, IntPtr.Zero);
        return found;
    }

    static void Put(IntPtr update, int type, Name name, byte[] data) {
        IntPtr pointer = name.Pointer();
        try {
            if (!UpdateResource(update, (IntPtr)type, pointer, name.Lang, data, data == null ? 0u : (uint)data.Length))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "UpdateResource");
        } finally {
            if (name.Text != null) Marshal.FreeHGlobal(pointer);
        }
    }

    // One block of a version resource: its length, the length of its value,
    // its kind (1: text, 0: bytes), its key, the value and its children,
    // each part starting on a multiple of four bytes.
    static byte[] Block(string key, byte[] value, bool text, params byte[][] children) {
        var body = new MemoryStream();
        byte[] name = Encoding.Unicode.GetBytes(key + "\0");
        body.Write(name, 0, name.Length);
        Pad(body, 6);
        body.Write(value, 0, value.Length);
        foreach (byte[] child in children) {
            Pad(body, 6);
            body.Write(child, 0, child.Length);
        }
        var block = new MemoryStream();
        var writer = new BinaryWriter(block);
        writer.Write((ushort)(6 + body.Length));
        // A text's length counts characters, with the NUL at its end.
        writer.Write((ushort)(text ? value.Length / 2 : value.Length));
        writer.Write((ushort)(text ? 1 : 0));
        writer.Write(body.ToArray());
        return block.ToArray();
    }

    // Fill up so that the block, whose header of `header` bytes comes
    // before `body`, goes on at a multiple of four bytes.
    static void Pad(MemoryStream body, int header) {
        while ((header + body.Length) % 4 != 0) body.WriteByte(0);
    }

    static byte[] Text(string key, string value) {
        return Block(key, Encoding.Unicode.GetBytes(value + "\0"), true);
    }

    // The version resource: the fixed part with the numbers, the texts for
    // US English in Unicode, and the note which language that is.
    static byte[] VersionInfo(string version) {
        var parts = new ushort[4];
        string[] given = version.Split('-')[0].Split('.');
        for (int i = 0; i < 4 && i < given.Length; i++) ushort.TryParse(given[i], out parts[i]);
        var fixedInfo = new MemoryStream();
        var writer = new BinaryWriter(fixedInfo);
        uint high = ((uint)parts[0] << 16) | parts[1], low = ((uint)parts[2] << 16) | parts[3];
        writer.Write(0xFEEF04BDu);   // signature
        writer.Write(0x00010000u);   // structure version
        writer.Write(high); writer.Write(low);   // file version
        writer.Write(high); writer.Write(low);   // product version
        writer.Write(0x3Fu);         // flags that are valid
        writer.Write(0u);            // flags: a release build
        writer.Write(0x00040004u);   // for Windows NT
        writer.Write(1u);            // an application
        writer.Write(0u);            // no subtype
        writer.Write(0u); writer.Write(0u);   // no date
        byte[] strings = Block("StringFileInfo", new byte[0], true,
            Block("040904B0", new byte[0], true,
                // This file is ASCII (Windows PowerShell reads it as ANSI): the
                // a-umlaut and the copyright sign are given by their numbers.
                Text("CompanyName", "StackForge UG (haftungsbeschr" + (char)0xE4 + "nkt)"),
                Text("FileDescription", "Magnetite"),
                Text("FileVersion", version),
                Text("InternalName", "magnetite"),
                Text("LegalCopyright", "Copyright " + (char)0xA9 + " 2026 the Magnetite contributors"),
                Text("OriginalFilename", "magnetite.exe"),
                Text("ProductName", "Magnetite"),
                Text("ProductVersion", version)));
        byte[] languages = Block("VarFileInfo", new byte[0], true,
            Block("Translation", new byte[] { 0x09, 0x04, 0xB0, 0x04 }, false));
        return Block("VS_VERSION_INFO", fixedInfo.ToArray(), false, strings, languages);
    }

    public static int Apply(string exe, string ico, string version) {
        byte[] file = File.ReadAllBytes(ico);
        if (file.Length < 6 || BitConverter.ToUInt16(file, 0) != 0 || BitConverter.ToUInt16(file, 2) != 1)
            throw new InvalidDataException("not an .ico file: " + ico);
        int count = BitConverter.ToUInt16(file, 4);

        IntPtr module = LoadLibraryEx(exe, IntPtr.Zero, LOAD_LIBRARY_AS_DATAFILE);
        if (module == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "LoadLibraryEx");
        List<Name> icons, groups, versions;
        try {
            icons = Existing(module, RT_ICON);
            groups = Existing(module, RT_GROUP_ICON);
            versions = Existing(module, RT_VERSION);
        } finally {
            FreeLibrary(module);
        }
        if (groups.Count == 0) groups.Add(new Name { Id = 1, Lang = ENGLISH_US });

        // The group resource is the .ico's directory with each image's file
        // offset (4 bytes) replaced by its resource id (2 bytes).
        var group = new MemoryStream();
        group.Write(file, 0, 6);
        var images = new List<byte[]>();
        for (int i = 0; i < count; i++) {
            int entry = 6 + 16 * i;
            int size = BitConverter.ToInt32(file, entry + 8);
            int offset = BitConverter.ToInt32(file, entry + 12);
            var image = new byte[size];
            Array.Copy(file, offset, image, 0, size);
            images.Add(image);
            group.Write(file, entry, 12);
            group.Write(BitConverter.GetBytes((ushort)(i + 1)), 0, 2);
        }

        IntPtr update = BeginUpdateResource(exe, false);
        if (update == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "BeginUpdateResource");
        try {
            foreach (Name old in icons) Put(update, RT_ICON, old, null);
            for (int i = 0; i < count; i++)
                Put(update, RT_ICON, new Name { Id = (ushort)(i + 1), Lang = ENGLISH_US }, images[i]);
            foreach (Name g in groups) Put(update, RT_GROUP_ICON, g, group.ToArray());
            // One version resource, ours: Windows reads the first it finds.
            foreach (Name old in versions) Put(update, RT_VERSION, old, null);
            Put(update, RT_VERSION, new Name { Id = 1, Lang = ENGLISH_US }, VersionInfo(version));
        } catch {
            EndUpdateResource(update, true);
            throw;
        }
        if (!EndUpdateResource(update, false)) throw new Win32Exception(Marshal.GetLastWin32Error(), "EndUpdateResource");
        return groups.Count;
    }
}
'@

$groups = [Brand]::Apply((Resolve-Path $Exe).Path, (Resolve-Path $Ico).Path, $Version)
$now = (Get-Item $Exe).VersionInfo
Write-Output "branded $Exe ($groups icon group(s)): $($now.FileDescription) $($now.FileVersion)"
