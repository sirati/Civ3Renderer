# Input runtime assertions

Source 31b5951a200234422c73a7c8c8f8b97bea77424fc8c9a86d2580003553c3ef58. All cases await coordinated runtime execution. These are expected outcomes, not pass claims.

| Case | Procedure | Expected |
|---|---|---|
| Free OS cursor | Move pointer over map and buttons while GPU busy | Pointer motion and icon changes continue independently of newly presented frames |
| Blocked GPU | Debugger-suspend only viewer-gpu after identifying thread, then resume | Old frame remains; OS cursor moves and changes icon; renderer recovers |
| Remote target | G beyond native camera, valid-invalid-valid; repeat other available mission | Actual destination determines validity; only modes 1 and 0x12 snap; invalid uses cell 5 |
| Long Go To | Target distant visible tile | Path, turn count, distant white outline remain visible and update |
| rightbuttondrag-confirm66 | Drag mode right-drag/release, move elsewhere, left original tile. Repeat another tile, selection or dialog | Released preview fixed; same tile/unit/world confirms once; changed identity cancels without stale move or dialog close |
| Capture axes | Pure X and pure Y motion against independent barriers | Blocked axis does not discard other-axis motion or produce diagonal drift |
| Sensitivity | Same relative displacement at 0.5, 1, 2.5 with same zoom | Accumulated world displacement follows sensitivity ratio; free pointer unchanged |
| Equal selection focus | Select same unit twice, open menu, move across panes | No unintended focus jump; menu retains input ownership |
| Multiple windows | Lose focus while captured or Alt-editing, return | Capture and held keys release; edit canceled; no latent command |
| Alt cancellation / Tab | Alt alone on menu/link/slider; Escape or focus loss, then Tab/arrows | Mode cancels; slider receives Cancel on focus loss; no stale Alt state; deselected slider does not absorb arrows |

Mac sole GUI operator is vm_snapshot_sol. Mac software Vulkan + bitmap proves behavior only. Host AMD GPU runtime awaits mac_fps_music coordination. No deliberate stall control exists in current environment-variable interfaces; process-wide SIGSTOP cannot establish independent cursor/event-loop behavior.
