param([string]$Seq = "caps+,caps-", [int]$GapMs = 30, [switch]$Batch, [switch]$NoRestore, [string]$Layout = "")
$src = @'
using System;
using System.Runtime.InteropServices;
using System.Threading;
using System.Collections.Generic;
public static class Probe {
  public delegate IntPtr HookProc(int code, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")] static extern IntPtr SetWindowsHookEx(int id, HookProc proc, IntPtr hMod, uint tid);
  [DllImport("user32.dll")] static extern bool UnhookWindowsHookEx(IntPtr h);
  [DllImport("user32.dll")] static extern IntPtr CallNextHookEx(IntPtr h, int code, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern bool PeekMessage(out MSG m, IntPtr h, uint a, uint b, uint f);
  [DllImport("user32.dll")] static extern bool TranslateMessage(ref MSG m);
  [DllImport("user32.dll")] static extern IntPtr DispatchMessage(ref MSG m);
  [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] inputs, int size);
  [DllImport("user32.dll")] static extern short GetAsyncKeyState(int vk);
  [DllImport("user32.dll")] static extern short GetKeyState(int vk);
  [StructLayout(LayoutKind.Sequential)] public struct MSG { public IntPtr hwnd; public uint message; public IntPtr wParam; public IntPtr lParam; public uint time; public int x; public int y; }
  [StructLayout(LayoutKind.Sequential)] public struct KBDLLHOOKSTRUCT { public uint vkCode; public uint scanCode; public uint flags; public uint time; public UIntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public UIntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public UIntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Explicit)] public struct INPUTUNION { [FieldOffset(0)] public MOUSEINPUT mi; [FieldOffset(0)] public KEYBDINPUT ki; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public INPUTUNION u; }
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern IntPtr LoadKeyboardLayoutW(string klid, uint flags); [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l); [DllImport("user32.dll")] static extern IntPtr GetKeyboardLayout(uint tid); [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr pid);
  public static string SwitchLayout(string klid) { IntPtr hkl = LoadKeyboardLayoutW(klid, 0); IntPtr fg = GetForegroundWindow(); PostMessageW(fg, 0x50, IntPtr.Zero, hkl); Pump(400); return string.Format("requested=0x{0:X} foreground-thread layout now=0x{1:X}", (long)hkl, (long)GetKeyboardLayout(GetWindowThreadProcessId(fg, IntPtr.Zero))); }
  static HookProc proc;
  public static List<string> Log = new List<string>();
  static long t0 = 0;
  static IntPtr Cb(int code, IntPtr w, IntPtr l) {
    if (code >= 0) {
      KBDLLHOOKSTRUCT k = (KBDLLHOOKSTRUCT)Marshal.PtrToStructure(l, typeof(KBDLLHOOKSTRUCT));
      int m = (int)w;
      string dir = (m == 0x101 || m == 0x105) ? "UP  " : "DOWN";
      ulong extra = (ulong)k.dwExtraInfo;
      string src = extra == 0x57524D00 ? "REMAP" : extra == 0x57524D01 ? "COMP " : (k.flags & 0x10) != 0 ? "INJ  " : "PHYS ";
      if (t0 == 0) t0 = k.time;
      Log.Add(string.Format("+{0,4}ms {1} {2} vk=0x{3:X2} sc=0x{4:X2} flags=0x{5:X2}", k.time - t0, src, dir, k.vkCode, k.scanCode, k.flags));
    }
    return CallNextHookEx(IntPtr.Zero, code, w, l);
  }
  static INPUT Key(ushort vk, ushort sc, bool up) {
    INPUT i = new INPUT(); i.type = 1; i.u.ki.wVk = vk; i.u.ki.wScan = sc; i.u.ki.dwFlags = up ? 2u : 0u; return i;
  }
  public static uint Send(INPUT[] arr) { return SendInput((uint)arr.Length, arr, Marshal.SizeOf(typeof(INPUT))); }
  public static INPUT Tok(string t) {
    bool up = t.EndsWith("-"); string k = t.TrimEnd('+', '-');
    switch (k) {
      case "caps": return Key(0x14, 0x3A, up);
      case "a": return Key(0x41, 0x1E, up);
      case "ctrl": return Key(0xA2, 0x1D, up);
      case "shift": return Key(0xA0, 0x2A, up);
      case "alt": return Key(0xA4, 0x38, up);
      case "f7": return Key(0x76, 0x41, up);
      case "scroll": return Key(0x91, 0x46, up);
      default: throw new Exception("bad token " + t);
    }
  }
  public static void Pump(int ms) {
    long end = Environment.TickCount + ms;
    while (Environment.TickCount < end) { MSG m; while (PeekMessage(out m, IntPtr.Zero, 0, 0, 1)) { TranslateMessage(ref m); DispatchMessage(ref m); } Thread.Sleep(5); }
  }
  static IntPtr hook;
  public static void Install() { proc = Cb; hook = SetWindowsHookEx(13, proc, IntPtr.Zero, 0); }
  public static void Uninstall() { UnhookWindowsHookEx(hook); }
  public static int CapsToggle() { return GetKeyState(0x14) & 1; }
  public static string State() {
    return string.Format("LCtrl async={0} sync={1} | LShift={2} | Caps toggle={3}",
      (GetAsyncKeyState(0xA2) & 0x8000) != 0 ? "DOWN" : "up", (GetKeyState(0xA2) & 0x8000) != 0 ? "DOWN" : "up",
      (GetAsyncKeyState(0xA0) & 0x8000) != 0 ? "DOWN" : "up", CapsToggle());
  }
}
'@
Add-Type -TypeDefinition $src
[Probe]::Install()
[Probe]::Pump(200)
if ($Layout) { "layout: " + [Probe]::SwitchLayout($Layout) }
$caps0 = [Probe]::CapsToggle()
"seq: $Seq  (gap ${GapMs}ms, batch=$Batch)"
"before: " + [Probe]::State()
$toks = $Seq.Split(",") | ForEach-Object { $_.Trim() } | Where-Object { $_ }
if ($Batch) {
  $arr = [Probe+INPUT[]]@($toks | ForEach-Object { [Probe]::Tok($_) })
  "sent=" + [Probe]::Send($arr) + "/" + $arr.Length
} else {
  foreach ($t in $toks) { $n = [Probe]::Send(@([Probe]::Tok($t))); if ($n -ne 1) { "SendInput($t) returned $n" }; [Probe]::Pump($GapMs) }
}
[Probe]::Pump(600)
"after:  " + [Probe]::State()
[Probe]::Log
if (([Probe]::State()) -like "*LCtrl async=DOWN*") { "cleanup: releasing stuck LCtrl"; [Probe]::Send(@([Probe]::Tok("ctrl-"))); [Probe]::Pump(200) }
if (([Probe]::State()) -like "*LShift=DOWN*") { "cleanup: releasing stuck LShift"; [Probe]::Send(@([Probe]::Tok("shift-"))); [Probe]::Pump(200) }
if (-not $NoRestore -and [Probe]::CapsToggle() -ne $caps0) { "cleanup: caps toggle changed, tapping CapsLock"; [Probe]::Send(@([Probe]::Tok("caps+"))); [Probe]::Pump(30); [Probe]::Send(@([Probe]::Tok("caps-"))); [Probe]::Pump(200) }
"final:  " + [Probe]::State()
if ($Layout) { "layout back: " + [Probe]::SwitchLayout("00000411") }
[Probe]::Uninstall()
