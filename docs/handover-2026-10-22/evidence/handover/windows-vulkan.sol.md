# Native Windows Vulkan validation

Owned throwaway VM c3r-vulkan-sol, 2 GiB and 4 vCPU, launched under c3r-vms.slice/c3r-windows-vulkan-sol.scope. Actual /proc QEMU cgroup verified before tests. Shared cap unchanged at 10,000,000,000 bytes, swap 0. Other VMs untouched. No source edits, game assets or GUI operations.

Existing artifact search found Wine loader shims but no usable native 32-bit software Vulkan ICD. Obtained trusted prebuilt x86 SwiftShader and native Vulkan loader from Google's official Chromium snapshot storage:
https://storage.googleapis.com/chromium-browser-snapshots/Win/1722657/chrome-win.zip
Revision selected by official Win/LAST_CHANGE,1722657. Extracted only vk_swiftshader.dll, vk_swiftshader_icd.json and vulkan-1.dll. PE headers confirm i386. Official archive and all extracted-file SHA256 in windows-vulkan-sol/evidence/driver-sha256.txt; import/PE details driver-pe.log. No driver compilation or system installation.

Guest loader is app-local in owned C:\c3r-tests; ICD and JSON under C:\c3r-tests\driver. Every invocation uses process-only WGPU_BACKEND=vulkan, VK_DRIVER_FILES and VK_ICD_FILENAMES pointing exactly at owned ICD JSON. No DX12 fallback permitted. Actual adapter/backend subsequently confirmed as SwiftShader Device (Subzero), Vulkan, as recorded below.

Release owner confirms final source 46eec60b0f736fd994ac95f63cc82b50e8ee3bc0ed95451ce89b64fd9886f89b and shader c1ae7f49a190658a9f579382035051041f062bdb3d15f401c76f4077bc8b18ab. Reused retained executables are labeled by actual prior scratch/frozen source provenance, not assumed to be final deployment binaries. First adapter probe is the retained original ef renderer gradient diagnostic; focused suites will use retained final FP16 scratch executables with matching c1ae shader. Final paired deployment proceeds independently.

## Verified Vulkan results

Actual native Windows adapter: SwiftShader Device (Subzero), vendor 6880/device 49374, CPU, SwiftShader driver, backend Vulkan. Confirmed directly by independent raw blend diagnostic and again by the complete GPU reference suite. Process-only Vulkan backend and exact ICD path supplied to every invocation; no DX12 fallback. Software Vulkan correctness coverage, not physical GPU performance coverage.

Exact frozen final46e source test binaries were supplied by release owner with original hashes/protocol 47/canonical c1ae shader manifest. Debug sections stripped only in owned transport copies; .text byte hashes remain identical and each stripped final binary still embeds canonicalSPIR-V exactly once. Original hashes verified against release manifest; executable-provenance.json records original/transport/code/shader identities.

Exact final46e Vulkan checks:
- bridge tiny_background_masks_preserve_trimmed_nominal_pixels_at_fractional_phases_and_opacity passed 1, 403 filtered, 82.04 s;
- renderer retained_fp16_texture_bytes_are_reserved_before_atlas_transfer passed 1, 24 filtered, 0.04 s.
Logs final46e-tiny-mask.log and final46e-memory-cap.log, both exit 0, output not truncated.

Companion retained FP16 scratch-source executables with the same canonical c1ae shader, separately labeled from exact final46e artifacts:
- focused world, scaled world, interface view and quantized world each passed 1;
- quantized_blending full independent exhaustive suite passed 2, including successive integer-boundary oracle;
- text_at_target synthetic suite passed 3, no font or game-asset fixture;
- broadened full gpu_matches_reference suite passed 38/38,73.67s.
Independent raw WGSL blend oracle also matches all 2,048 pixels on SwiftShader with the ordinary Vulkan path. Earlier WARP-specific blend differences are absent here. Full GPU suite itself prints the same Vulkan adapter identity. These retained companion binaries are NOT claimed to have been rebuilt from final46e.

## Transport and reproducibility

SSH/PowerShell startup and verbose logging probe routes did not provide usable results in this fresh clone. A minimal direct QGA runner and explicit per-process environment succeeded. Direct QGA file transfer avoided SSH; no system installation or registry ICD registration. Preliminary transport failures remain in evidence and are not counted as rendering-test failures or successes. Complete counted-test status objects show output not truncated.

Strip command: i686-w64-mingw32-strip --strip-debug -o OWNED_COPY ORIGINAL_RETAINED_EXE. No source compilation or edits. Test selectors, driver hashes, exact source manifest and per-executable .text identities retained in evidence. Driver source is the official immutable Chromium revision URL above; no third-party driver binaries or long driver build used.


## Cleanup complete

Only owned c3r-vulkan-sol VM and ephemeral overlay destroyed; its scope is inactive. Host driver archive, extracted DLLs/ICD scratch and stripped transport executables removed after validating exact owned paths. Original retained artifacts and all evidence remain. Shared MemoryMax is still 10,000,000,000 bytes with swap 0; final current memory 3,267,162,112 bytes. Other VMs and golden image untouched. No source edits or game assets used. Cleanup evidence: vm-cleanup.log, host-cleanup.log and cgroup-final.log. Final log hashes: evidence/log-sha256.txt.
