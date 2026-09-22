param([string]$Seq = "ctrl+,caps+,caps-,ctrl-", [int]$GapMs = 30)
$src = @'
using System;
using System.Runtime.InteropServices;
using System.Threading;
using System.Collections.Generic;
public static class Raw {
  public delegate IntPtr WndProc(IntPtr h, uint m, IntPtr w, IntPtr l);
  public delegate IntPtr HookProc(int code, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] static extern IntPtr SetWindowsHookEx(int id, HookProc proc, IntPtr hMod, uint tid);
  [DllImport("user32.dll")] static extern bool UnhookWindowsHookEx(IntPtr h);
  [DllImport("user32.dll")] static extern IntPtr CallNextHookEx(IntPtr h, int code, IntPtr w, IntPtr l);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern ushort RegisterClassExW(ref WNDCLASSEX c);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr CreateWindowExW(uint ex, string cls, string name, uint style, int x, int y, int w, int h, IntPtr parent, IntPtr menu, IntPtr inst, IntPtr param);
  [DllImport("user32.dll")] static extern IntPtr DefWindowProcW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern bool RegisterRawInputDevices(RAWINPUTDEVICE[] d, uint n, uint size);
  [DllImport("user32.dll")] static extern uint GetRawInputData(IntPtr h, uint cmd, IntPtr data, ref uint size, uint hdr);
  [DllImport("user32.dll")] static extern bool PeekMessage(out MSG m, IntPtr h, uint a, uint b, uint f);
  [DllImport("user32.dll")] static extern bool TranslateMessage(ref MSG m);
  [DllImport("user32.dll")] static extern IntPtr DispatchMessage(ref MSG m);
  [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] inputs, int size);
  [DllImport("user32.dll")] static extern short GetAsyncKeyState(int vk);
  [DllImport("user32.dll")] static extern short GetKeyState(int vk);
  [DllImport("kernel32.dll")] static extern IntPtr GetModuleHandleW(IntPtr n);
  [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)] public struct WNDCLASSEX { public uint cbSize; public uint style; public IntPtr lpfnWndProc; public int cbClsExtra; public int cbWndExtra; public IntPtr hInstance; public IntPtr hIcon; public IntPtr hCursor; public IntPtr hbrBackground; public string lpszMenuName; public string lpszClassName; public IntPtr hIconSm; }
  [StructLayout(LayoutKind.Sequential)] public struct RAWINPUTDEVICE { public ushort usUsagePage; public ushort usUsage; public uint dwFlags; public IntPtr hwndTarget; }
  [StructLayout(LayoutKind.Sequential)] public struct MSG { public IntPtr hwnd; public uint message; public IntPtr wParam; public IntPtr lParam; public uint time; public int x; public int y; }
  [StructLayout(LayoutKind.Sequential)] public struct KBDLLHOOKSTRUCT { public uint vkCode; public uint scanCode; public uint flags; public uint time; public UIntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public UIntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public UIntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Explicit)] public struct INPUTUNION { [FieldOffset(0)] public MOUSEINPUT mi; [FieldOffset(0)] public KEYBDINPUT ki; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public INPUTUNION u; }
  static WndProc wp; static HookProc hp; static IntPtr hook;
  public static List<string> Log = new List<string>();
  static long t0 = 0;
  static long T() { long t = Environment.TickCount; if (t0 == 0) t0 = t; return t - t0; }
  static IntPtr Proc(IntPtr h, uint m, IntPtr w, IntPtr l) {
    if (m == 0x00FF) {
      uint size = 0; GetRawInputData(l, 0x10000003, IntPtr.Zero, ref size, 24);
      IntPtr buf = Marshal.AllocHGlobal((int)size);
      if (GetRawInputData(l, 0x10000003, buf, ref size, 24) == size) {
        uint type = (uint)Marshal.ReadInt32(buf, 0);
        if (type == 1) {
          ushort make = (ushort)Marshal.ReadInt16(buf, 24); ushort flags = (ushort)Marshal.ReadInt16(buf, 26);
          ushort vk = (ushort)Marshal.ReadInt16(buf, 30); uint msg = (uint)Marshal.ReadInt32(buf, 32);
          Log.Add(string.Format("+{0,4}ms RAW  {1} make=0x{2:X2} vk=0x{3:X2} msg=0x{4:X3}", T(), (flags & 1) != 0 ? "BREAK" : "MAKE ", make, vk, msg));
        }
      }
      Marshal.FreeHGlobal(buf);
      return IntPtr.Zero;
    }
    return DefWindowProcW(h, m, w, l);
  }
  static IntPtr Cb(int code, IntPtr w, IntPtr l) {
    if (code >= 0) {
      KBDLLHOOKSTRUCT k = (KBDLLHOOKSTRUCT)Marshal.PtrToStructure(l, typeof(KBDLLHOOKSTRUCT));
      int m = (int)w; string dir = (m == 0x101 || m == 0x105) ? "UP  " : "DOWN";
      Log.Add(string.Format("+{0,4}ms HOOK {1} vk=0x{2:X2} sc=0x{3:X2}", T(), dir, k.vkCode, k.scanCode));
    }
    return CallNextHookEx(IntPtr.Zero, code, w, l);
  }
  public static void Install() {
    wp = Proc; hp = Cb;
    WNDCLASSEX c = new WNDCLASSEX(); c.cbSize = (uint)Marshal.SizeOf(typeof(WNDCLASSEX)); c.lpfnWndProc = Marshal.GetFunctionPointerForDelegate(wp); c.hInstance = GetModuleHandleW(IntPtr.Zero); c.lpszClassName = "RawProbeCls";
    RegisterClassExW(ref c);
    IntPtr hwnd = CreateWindowExW(0, "RawProbeCls", "rawprobe", 0, 0, 0, 0, 0, new IntPtr(-3), IntPtr.Zero, c.hInstance, IntPtr.Zero);
    if (hwnd == IntPtr.Zero) Log.Add("CreateWindowEx FAILED");
    RAWINPUTDEVICE[] d = new RAWINPUTDEVICE[1]; d[0].usUsagePage = 1; d[0].usUsage = 6; d[0].dwFlags = 0x100; d[0].hwndTarget = hwnd;
    if (!RegisterRawInputDevices(d, 1, (uint)Marshal.SizeOf(typeof(RAWINPUTDEVICE)))) Log.Add("RegisterRawInputDevices FAILED");
    hook = SetWindowsHookEx(13, hp, IntPtr.Zero, 0);
  }
  public static void Uninstall() { UnhookWindowsHookEx(hook); }
  static INPUT Key(ushort vk, ushort sc, bool up) { INPUT i = new INPUT(); i.type = 1; i.u.ki.wVk = vk; i.u.ki.wScan = sc; i.u.ki.dwFlags = up ? 2u : 0u; return i; }
  public static uint Send(INPUT[] arr) { return SendInput((uint)arr.Length, arr, Marshal.SizeOf(typeof(INPUT))); }
  public static INPUT Tok(string t) {
    bool up = t.EndsWith("-"); string k = t.TrimEnd('+', '-');
    switch (k) { case "caps": return Key(0x14, 0x3A, up); case "a": return Key(0x41, 0x1E, up); case "ctrl": return Key(0xA2, 0x1D, up); case "shift": return Key(0xA0, 0x2A, up); default: throw new Exception("bad token " + t); }
  }
  public static void Pump(int ms) { long end = Environment.TickCount + ms; while (Environment.TickCount < end) { MSG m; while (PeekMessage(out m, IntPtr.Zero, 0, 0, 1)) { TranslateMessage(ref m); DispatchMessage(ref m); } Thread.Sleep(5); } }
  public static int CapsToggle() { return GetKeyState(0x14) & 1; }
  public static string State() { return string.Format("LCtrl async={0} | Caps toggle={1}", (GetAsyncKeyState(0xA2) & 0x8000) != 0 ? "DOWN" : "up", CapsToggle()); }
}
'@
Add-Type -TypeDefinition $src
[Raw]::Install(); [Raw]::Pump(200)
$caps0 = [Raw]::CapsToggle()
"seq: $Seq"; "before: " + [Raw]::State()
foreach ($t in $Seq.Split(",")) { $t = $t.Trim(); if ($t) { [Raw]::Send(@([Raw]::Tok($t))) | Out-Null; [Raw]::Pump($GapMs) } }
[Raw]::Pump(600)
"after:  " + [Raw]::State()
[Raw]::Log
if (([Raw]::State()) -like "*LCtrl async=DOWN*") { "cleanup: releasing LCtrl"; [Raw]::Send(@([Raw]::Tok("ctrl-"))) | Out-Null; [Raw]::Pump(200) }
if ([Raw]::CapsToggle() -ne $caps0) { "cleanup: caps toggle changed, tapping CapsLock"; [Raw]::Send(@([Raw]::Tok("caps+"))) | Out-Null; [Raw]::Pump(30); [Raw]::Send(@([Raw]::Tok("caps-"))) | Out-Null; [Raw]::Pump(200) }
"final:  " + [Raw]::State()
[Raw]::Uninstall()
