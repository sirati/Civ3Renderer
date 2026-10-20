# Pointer runtime probe

`pointer_qa` tests the common pointer controller on an isolated graphical guest.
It uses winit on its main thread and has no platform-specific code or GPU
requirements. Do not run it on the host user's desktop.

Build and run in the guest:

```sh
cargo build --locked -p pointer-control --example pointer_qa
./target/debug/examples/pointer_qa --run
```

The window is 800 by 600 physical pixels. Its `READY` line gives the actual
content origin, size and scale factor. Supply commands on stdin, one per line.
For SSH automation, redirect stdin from a FIFO and keep the writer open until
`quit`. Output flushes after every line.

1. Send `case bounds`, then inject a small real pointer movement to obtain a fresh
   native cursor sample. A warp alone may not emit a native movement event.
2. Send `arm` to clear observations after that fresh sample. It rejects arming
   while the last native sample is invalid.
3. Inject real pointer motion through the guest console, VNC or QMP. Send
   `report` to record observations, then continue to the next case.
4. Send `quit` when finished. Escape also releases the pointer and exits.

| Case | Physical content geometry | Checks |
| --- | --- | --- |
| `bounds` | x=80..719, y=80..519 | Motion towards all four boundaries stops inside. Large diagonal motion stays inside. |
| `horizontal` | x=80..719, y=300 | Vertical and diagonal motion keeps y within the one-pixel row. Horizontal motion remains possible. |
| `vertical` | x=400, y=80..519 | Horizontal and diagonal motion keeps x within the one-pixel column. Vertical motion remains possible. |
| `divider` | Outer x=40..759, y=40..559. Divider x=390..409 except opening y=260..339. | Crossing at y=200 or y=400 stops at the divider. Crossing at y=300 succeeds in both directions. Fast motion cannot jump through the divider. |
| `strip` | Outer bounds with top y=40..139 excluded | Upward and diagonal motion stops below y=140. The pointer can move along the strip's lower edge. |
| `box` | Outer bounds with x=300..499, y=160..279 excluded | Motion stops at all four box faces. Diagonal and fast motion cannot jump through the box. Motion can pass around it. |
| `none` | No region | The pointer can leave the window. |

`CURSOR` records the actual native `CursorMoved` position, including subpixels.
`RAW` records device deltas separately. `REPORT` includes native confinement
status, observation count, out-of-region count, straight-segment crossing candidate count, the last
native cursor sample, and that sample's age. Zero samples are not a pass. A
backend with `native_confined=false` has not supplied native region enforcement;
this example does not replace missing native barriers with application warps.
In particular, the current X11 backend only provides full-window grabs and
explicit pointer movement.

`move X Y` requests an intentional pointer placement through the common API.
Use it to position between separate measurements, then `arm` after the native
sample arrives. Do not use intentional warps to test barriers, since the
application is allowed to move a pointer across them. The tool also flags
straight segments between observed samples that cross an exclusion. These are
trace-review candidates, not proof of a barrier failure. A backend can slide
around an exclusion corner within one native event, so endpoint positions do
not establish the actual path. Use repeated small injections and observe each
blocked face to test fast crossing behavior. Every intentional placement clears
the native sample and disarms measurement until fresh injected motion arrives.

For capture and lifecycle checks:

- Send `capture on`, inject mouse motion, then send `report`. The requested
  capture result identifies locked or confined behavior. Region observations
  suspend while captured; raw deltas continue to be logged.
- Send `capture off`. Region confinement resumes when the window is focused and
  the pointer is inside. Recheck the horizontal and vertical cases after capture
  release, which exercises the single-axis edge handoff used by the viewer.
- Switch focus to another guest application or hide the probe. A `FOCUS false`
  line must be followed by `RELEASE`. Move outside the probe to confirm that the
  guest desktop remains usable. Return focus and check that capture was not
  silently restored.
- Invoke the standard native application menu using the guest's keyboard menu
  shortcut while confined or captured. Move within the menu and close it. Record
  the native cursor/raw event lines and verify that selecting a menu item works.
  Menu tracking may not produce a window focus event, so an empty focus log
  cannot by itself prove that menu release worked.
- Close the window and verify `RELEASE` and process exit. Do the same with
  `quit`; the example never grabs another process's window.

This is functional evidence for the selected guest backend. Software pointer
injection and virtual display scale do not substitute for later hardware tests
on the target machine.
