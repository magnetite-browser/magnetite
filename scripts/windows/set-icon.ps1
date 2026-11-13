# Replace the icon of a Windows executable with the images of an .ico file.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File set-icon.ps1 <exe> <ico>
#
# scripts/build.sh uses it on the package's magnetite.exe: that file is CEF's
# bootstrap.exe and would show CEF's icon in Explorer and the taskbar. The
# icon groups the executable already has keep their names (the program loads
# its window icon by name); their images are replaced by ours.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [Parameter(Mandatory = $true)][string]$Ico
)
$ErrorActionPreference = 'Stop'

Add-Type @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;

public static class IconPatch {
    const int RT_ICON = 3, RT_GROUP_ICON = 14;
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

    public static int Apply(string exe, string ico) {
        byte[] file = File.ReadAllBytes(ico);
        if (file.Length < 6 || BitConverter.ToUInt16(file, 0) != 0 || BitConverter.ToUInt16(file, 2) != 1)
            throw new InvalidDataException("not an .ico file: " + ico);
        int count = BitConverter.ToUInt16(file, 4);

        IntPtr module = LoadLibraryEx(exe, IntPtr.Zero, LOAD_LIBRARY_AS_DATAFILE);
        if (module == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error(), "LoadLibraryEx");
        List<Name> icons, groups;
        try {
            icons = Existing(module, RT_ICON);
            groups = Existing(module, RT_GROUP_ICON);
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
        } catch {
            EndUpdateResource(update, true);
            throw;
        }
        if (!EndUpdateResource(update, false)) throw new Win32Exception(Marshal.GetLastWin32Error(), "EndUpdateResource");
        return groups.Count;
    }
}
'@

$groups = [IconPatch]::Apply((Resolve-Path $Exe).Path, (Resolve-Path $Ico).Path)
Write-Output "icon set in $Exe ($groups icon group(s))"
