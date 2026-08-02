# Runs inside the guest, in its own process, alongside probe-foreground-watch.
#
# An independent EVENT_SYSTEM_FOREGROUND client: it installs the same kind of
# hook WinRemap installs (SetWinEventHook, WINEVENT_OUTOFCONTEXT, all processes
# and threads) and writes down every foreground change it is told about.
#
# The point is the comparison, not the log. When WinRemap does not report a
# switch, exactly two things can be true - the system never delivered the
# event, or it delivered it and WinRemap did not act on it - and from inside
# WinRemap they look identical. A second client subscribed to the same event
# tells them apart, which is the same reason 00-uia-actuation exists
# (ADR 0064 section 3).
#
# It runs in a separate process on purpose: the probe has to keep working
# (switching windows, typing) while this one sits in a message pump, and an
# out-of-context hook is only delivered to a thread that pumps.
#
# ASCII only on purpose. PowerShell 5.1 reads a BOM-less UTF-8 script as CP932.
#
#   powershell -File foreground-listener.ps1 -OutPath C:\Test\x.txt -Seconds 180

param(
    [string]$OutPath = "C:\Test\watch-listener.txt",
    [int]$Seconds = 180
)

Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public class FgWatch {
  public delegate void WinEventProc(IntPtr hook, uint ev, IntPtr hwnd, int idObject, int idChild, uint thread, uint time);

  [DllImport("user32.dll")] static extern IntPtr SetWinEventHook(
      uint min, uint max, IntPtr hmod, WinEventProc cb, uint pid, uint tid, uint flags);
  [DllImport("user32.dll")] static extern bool UnhookWinEvent(IntPtr h);
  [DllImport("user32.dll")] static extern bool PeekMessageW(out MSG m, IntPtr hwnd, uint min, uint max, uint remove);
  [DllImport("user32.dll")] static extern bool TranslateMessage(ref MSG m);
  [DllImport("user32.dll")] static extern IntPtr DispatchMessageW(ref MSG m);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);

  [StructLayout(LayoutKind.Sequential)]
  public struct MSG { public IntPtr hwnd; public uint message; public IntPtr w; public IntPtr l; public uint time; public int x; public int y; }

  // Held in a static field because the delegate is the only thing keeping the
  // callback alive: hand it straight to SetWinEventHook and the GC collects it
  // while Windows still holds the pointer - which crashes at the first event,
  // long after the code that caused it has returned.
  static WinEventProc _cb;
  static StreamWriter _w;

  static void OnEvent(IntPtr hook, uint ev, IntPtr hwnd, int idObject, int idChild, uint thread, uint time) {
    if (idObject != 0) { return; }   // OBJID_WINDOW only; menus and carets also raise this event
    uint pid;
    GetWindowThreadProcessId(hwnd, out pid);
    string name = "?";
    try { name = Process.GetProcessById((int)pid).ProcessName + ".exe"; } catch { }
    _w.WriteLine(DateTime.Now.ToString("HH:mm:ss.fff") + "  " + name + "  hwnd=" + hwnd.ToInt64());
    _w.Flush();
  }

  public static void Run(string path, int seconds) {
    _w = new StreamWriter(path, false, new UTF8Encoding(false));
    _w.WriteLine(DateTime.Now.ToString("HH:mm:ss.fff") + "  listener started");
    _w.Flush();
    _cb = new WinEventProc(OnEvent);
    // 0x0003 = EVENT_SYSTEM_FOREGROUND, 0x0000 = WINEVENT_OUTOFCONTEXT.
    // Same arguments WinRemap uses (src/window.rs).
    IntPtr h = SetWinEventHook(0x0003, 0x0003, IntPtr.Zero, _cb, 0, 0, 0x0000);
    if (h == IntPtr.Zero) {
      _w.WriteLine("SetWinEventHook FAILED");
      _w.Flush();
      return;
    }
    DateTime end = DateTime.Now.AddSeconds(seconds);
    MSG m;
    while (DateTime.Now < end) {
      while (PeekMessageW(out m, IntPtr.Zero, 0, 0, 1)) {
        TranslateMessage(ref m);
        DispatchMessageW(ref m);
      }
      System.Threading.Thread.Sleep(15);
    }
    UnhookWinEvent(h);
    _w.WriteLine(DateTime.Now.ToString("HH:mm:ss.fff") + "  listener stopped");
    _w.Flush();
    _w.Close();
  }
}
'@

[FgWatch]::Run($OutPath, $Seconds)
