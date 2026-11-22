# Audio crate supply-chain vetting

Date of research: 2026-11-22. Scope: decode WAV/MP3, resample, output through CoreAudio / WASAPI / ALSA.
Everything here is point-in-time (crates.io, GitHub and the RustSec database as of 2026-11-21/22).
Nothing was built or run from downloaded sources; the project repo was not touched.

## 1. Bottom line

**Recommended minimal set (pure Rust except the platform API bindings):**

```toml
[dependencies]
symphonia = { version = "=0.6.1", default-features = false, features = ["wav", "pcm", "mp3"] }
cpal      = { version = "=0.18.2", default-features = false }
rubato    = { version = "=5.1.0", default-features = false }
# only if some game WAVs are IMA/MS-ADPCM (vetted, same owner, same repo, no new deps):
# symphonia = { ..., features = ["wav", "pcm", "mp3", "adpcm"] }
```

- 58 distinct crate-versions across the four targets (macOS 43, Windows 42 for both gnu and msvc, Linux 36). No crate on any of the three targets downloads anything at build time, and no crate runs fetched code.
- `cargo audit` (RustSec, 1555 advisories, DB updated 2026-11-21): **0 vulnerabilities, 0 warnings** for this set.
- Only **one** item in the whole set needs a conscious decision: **`smallvec`** (pulled in by `symphonia-core` and `symphonia-metadata`). The code of 1.16.6 is clean, but its maintainer set changed recently and the project is in heavy churn. Recommendation: `cargo update -p smallvec --precise 1.15.2` (see 4.2) and commit `Cargo.lock`.
- **Reject** `minimp3`, `minimp3_fixed` and their `slice-ring-buffer` dependency (RUSTSEC-2025-0044, unpatched double frees; also drags `winapi` + about 1,400 prebuilt import libs onto Windows).
- `rodio` and `kira` are not malicious as far as I can see, but their trees are about twice as big (77 to 84 crates per target against 36 to 43), duplicate `symphonia` 0.5.x next to 0.6.x, and hide the mixer and resampler details you want to control. Not recommended.

## 2. Verdict per candidate

| Candidate | Version checked | Verdict | Why |
|---|---|---|---|
| `symphonia` (features `wav`,`pcm`,`mp3`; default-features off) | 0.6.1 | **OK** | One long-standing owner (pdeljanov, since 2020-09). All six `symphonia*` crates in the core tree (`symphonia`, `-core`, `-bundle-mp3`, `-codec-pcm`, `-format-riff`, `-metadata`) have `#![forbid(unsafe_code)]` and no `build.rs`. Published crates are byte-identical to the repo commit. Its only non-trivial transitive deps are `smallvec`, `bytemuck`, `lazy_static`, `log`, `num-complex`, `regex-lite`, `bitflags`, `extended`. Bus-factor note: one account owns the whole family. |
| `symphonia-codec-adpcm` (optional) | 0.6.1 | **OK** | Same owner/repo; identical to repo; deps `log` + `symphonia-core` only; no `build.rs`. |
| `hound` | 3.5.1 | **OK** (redundant) | One owner (ruuda, since 2015). No dependencies, no `build.rs`. Matches repo. WAV only. Not needed if `symphonia` is used; fine as a zero-dependency fallback. |
| `minimp3` | 0.6.1 | **REJECT** | Depends on `slice-ring-buffer` 0.3.4 (RUSTSEC-2025-0044, no fixed version). Owner set changed in 2025-08 (phip1611 added, publisher of 0.6.x). C code. |
| `minimp3_fixed` | 0.5.4 | **REJECT** | One-person fork (BOB450; GitHub repo is a 1-star fork, last push 2023-10); same unpatched `slice-ring-buffer`; the "security patch" claim cannot be verified from the sources. Crate == repo (modulo CRLF). |
| `minimp3-sys` (the C binding both build on) | 0.6.0 / 0.3.2 | **CAUTION** | Vendored C is byte-identical to upstream `lieff/minimp3` at the pinned submodule commits (afb604c / ca39bce); `build.rs` is just `cc::Build`. Usable in principle, but it is C code and the pure-Rust `symphonia` makes it unnecessary. |
| `cpal` (default-features off) | 0.18.2 | **OK** | Owners: tomaka + `RustAudio/cpal-maintainers` team; releases are published by `rustaudio-publish-bot` (CI publishing). `build.rs` only reads `CPAL_ASIO_DIR`. No vendored C; platform glue only: `alsa` (+`alsa-sys` via pkg-config) on Linux, `coreaudio-rs` + `objc2-*` on macOS, `windows`/`windows-*` (Microsoft) on Windows. 196 `unsafe` lines in `cpal` itself (platform FFI, expected). |
| `rubato` (default-features off) | 5.1.0 | **OK** | One owner (HEnquist, since 2020; very active). Its deps (`audioadapter*`, `windowfunctions`, `visibility`, `audio-codec-algorithms`, `num-*`) are all by the same maintainer or well-known authors; all matched to repo. `unsafe` is limited to SIMD sinc kernels and an aligned buffer. Without `fft_resampler` you keep the `Async` sinc/polynomial resamplers and lose only `Fft`, and `realfft`/`rustfft` leave the tree. |
| `rodio` | 0.23.0 | **OK, not recommended** | Owner tomaka + `rodio-maintainers`; bot-published. Big tree (77 to 84 crates per target), `symphonia` 0.5.x + 0.6.x duplicate, `rand 0.10`/`chacha20`/`getrandom`, `encoding_rs`, `multiversion_no_op` (new), dbus path on Linux. |
| `kira` | 0.12.6 | **OK, not recommended** | Single owner (tesselode); `glam`, `rtrb`, `triple_buffer`, `atomic-arena`, `pastey` (paste fork by a small maintainer; reviewed, see table), dbus path via `cpal-realtime-dbus` default. |

## 3. How the checks were done (and where they stop)

Scratch projects under `/tmp/c3r-crate-vetting/{core,cmp,high}` (all exact-pinned latest versions) were resolved with Cargo (Nix rustc 1.99.1); `cargo tree -e normal,build` was run per target for `aarch64-apple-darwin`, `x86_64-pc-windows-gnu`, `x86_64-pc-windows-msvc`, `x86_64-unknown-linux-gnu` (trees in `/tmp/c3r-crate-vetting/trees/`).

1. **Owners / publishing**: crates.io API per crate (`owner_user`, `owner_team`, version list, downloads, yanked). crates.io exposes no ownership history, so "new owner" was inferred from the **publisher of each version and the date each publisher first appears**. Per-crate data: `table.tsv`; raw JSON: `meta/`. Account creation date of each publisher is in `table.tsv`.
2. **Integrity**: all 121 downloaded `.crate` files match the registry checksum (SHA-256). No resolved version is yanked.
3. **Repo match**: each crate's `.cargo_vcs_info.json` gives the exact commit. I shallow-fetched that commit (`git archive`, no checkout/hooks), exported the crate's subpath and diffed it against the published tarball (`compare/`). Also compared `Cargo.toml.orig` against the repo's `Cargo.toml`: **120/121 identical**. Differences found were only: CRLF line endings (`atomic-arena`, `kira`, `minimp3_fixed`), dev-only files present in the repo but absent from the crate, and vendored submodules (checked separately against upstream for `minimp3-sys`). Exceptions listed in section 5.
4. **`build.rs` / proc-macros**: all 19 build scripts and 8 proc-macro crates in the union tree were located; all build scripts were grepped for network, process, file-write and env access, and the small ones read in full. Only `proc-macro2`/`quote`/`thiserror`/`rustversion`/`libc` (and `autocfg`, used by `num-traits`) run `rustc` to probe versions (standard). Nothing touches the network or writes outside `OUT_DIR`. `visibility` (the only non-dtolnay, non-Microsoft proc-macro in the Linux/macOS core tree) was read in full: 84 lines, rewrites an item's visibility only. `libdbus-sys` can run `git submodule update` only under its non-default `vendored` feature (not enabled anywhere here).
5. **`-sys` / native code**: see table column "native"; in the recommended set only `alsa-sys` (links system `libasound` through `pkg-config`; no vendored C; its "native" hit is a `wrapper.h`) is relevant. `audio-codec-algorithms` ships two test-only 64 KB `.bin` tables (`#[cfg(test)]` + `include_bytes!`).
6. **Advisories**: `cargo audit` on all three scratch lockfiles (JSON in `audit/`), plus a name-by-name lookup of all 121 crates in the local RustSec database (the same data rustsec.org renders; no per-crate web lookups were made). GitHub Security Advisories were additionally checked for `smallvec`, because they are not mirrored in RustSec yet.
7. **Source pattern scan** of every core-set crate for network, process, env and filesystem use outside tests: `cpal` reads `PIPEWIRE_RUNTIME_DIR`/`XDG_RUNTIME_DIR` (inside its optional PipeWire host) and the PID for client names; `symphonia-core` implements `MediaSource` for `std::fs::File`; nothing else.
8. **Network use of this research**: only crates.io API, static.crates.io, GitHub public API/git for the crates' own repos, and a GitHub user lookup for one maintainer (`alejandro-vaz`). No project information was sent anywhere.

Not done (limits): no line-by-line audit of large crates (`cpal`, `symphonia-core`, `rubato`, `windows-*`, `objc2-*`); "crate == repo commit" does not prove the repo commit is benign; nothing was built or fuzzed; I cannot see who controls a maintainer's GitHub/crates.io account, only who published.

## 4. Findings you should know about

### 4.1 `slice-ring-buffer` 0.3.4 / `minimp3*` (reject)
RUSTSEC-2025-0044: four double-free bugs reachable from safe APIs, "No fixed upgrade is available". Both `minimp3` 0.6.1 and `minimp3_fixed` 0.5.4 depend on it, and on Windows it pulls `winapi` 0.3.9 and `winapi-x86_64-pc-windows-gnu` (about 1,416 prebuilt dlltool import libraries, 54 MB). `cargo audit` on the `cmp` lockfile fails with exactly this advisory.

### 4.2 `smallvec` (caution, required by `symphonia-core`/`symphonia-metadata` with `^1.13.1`)
- crates.io owners: SimonSapin, Ms2ger, **alejandro-vaz**, team `servo:cargo-publish`. `alejandro-vaz` first published on 2026-10-23 (1.16.4); earlier releases were by jdm and emilio (Servo members). Versions 1.16.6 and 2.0.0-beta.5 have no publisher user attached in the API (consistent with CI / trusted publishing; I could not confirm which).
- The GitHub repo moved from `servo/rust-smallvec` to `smallvec/smallvec`. `alejandro-vaz` (GitHub account created 2024-09-14, 17 public repos) is the repo's 2nd largest contributor (83 commits) and the only public member of the new org. In November 2026 there is a very high volume of commits from a handful of new contributors (v2 rewrite, "unify into a nightly feature", "remove fuzzer", lint and style refactors).
- Code review of the published artifacts: 1.16.6 == repo commit `347b5c3`; **no `build.rs`, no new dependencies, no network/process/FFI/file I/O** in the 1.15.2 to 1.16.6 diff (about 900 added lines, almost all in `lib.rs`; the added `unsafe` is retain/drain guards and uninitialised inline storage). I found nothing malicious.
- Open GitHub advisories (not yet in RustSec): GHSA-826h-qf7w-83mw (double free in `DrainFilter::keep_rest`, affects 1.11.0 to 1.15.1, needs a non-default feature, fixed in 1.15.2), GHSA-rpjf-r47f-8jh3 and GHSA-59p2-fqgc-fmqq (2.0.0 pre-releases only), GHSA-5h7v-3586-wm8c (never published). **None affects 1.16.6 or 1.15.2.**
- Recommendation: treat this as a governance risk, not a code finding. Conservative option: `cargo update -p smallvec --precise 1.15.2` (published 2026-06-11 by emilio, before the ownership change; patched for GHSA-826h; crate == repo commit `c469051`; manifest identical). Either way: never allow `2.0.0-*`, commit `Cargo.lock`, and re-diff on every bump.

### 4.3 Other flags (none blocking)
- `cpal` and `rodio` are published by the bot account `rustaudio-publish-bot` (CI publishing). Normal for the RustAudio org, but it means the org's CI credentials are part of the trust chain.
- `audioadapter-buffers`/`audioadapter-sample` list a repository URL that does not exist (`HEnquist/audioadapter-*-rs`); the sources are in `HEnquist/audioadapter-rs` at the recorded commit and are byte-identical there.
- `audio-codec-algorithms` (karip, created 2024) and `extended` (depp, 2022) are small single-maintainer crates with no (or no runtime) dependencies and zero `unsafe`; both read and match their repos.
- `multiversion_no_op` 1.0.0 (hsivonen, created 2026-07-30, 45M downloads already): brand-new but its whole source is an identity attribute macro. Only in the `rodio` tree.
- `pastey` 0.2.3 (AS1100K, small maintainer; fork of the unmaintained `paste`): only in `kira`'s tree; its single env-var read is the upstream `env!()` segment feature.
- `winapi-x86_64-pc-windows-gnu` 0.4.0: published 2018 without VCS info; the repo's rebuilt import libs differ (different embedded build path and, for 55 libs, different export lists), so it cannot be verified byte-for-byte. All members are plain dlltool objects. 494M downloads, owner retep998 since 2016. Irrelevant if `minimp3*` is avoided.
- Native PipeWire (`cpal` feature `pipewire`) would add about 45 crates incl. `bindgen`, `clang-sys`, `libspa-sys`, `pipewire-sys` and a libclang build requirement; I did not vet those. `cpal`'s `realtime` feature adds only `audio_thread_priority` (Mozilla) + `log` on Linux and `windows-sys` on Windows, nothing on macOS.

## 5. Full transitive dependency table (all 121 crate-versions in the union of the three scratch trees)

Legend. Target columns show which scratch set contains the crate on that target: **C** = recommended core (`symphonia`+`cpal`+`rubato`), **M** = comparison (`hound`, `minimp3`, `minimp3_fixed`), **H** = higher level (`rodio`+`kira`); `-` = not in any. **native** = the crate directory contains C/C++/headers/prebuilt libraries/binary blobs (not necessarily compiled). **Crate vs repo** = published tarball compared with the repo commit named in its `.cargo_vcs_info.json`. Owners are crates.io user logins plus teams. Per-crate notes behind each verdict are in `bin/verdicts.tsv`; the raw facts (downloads, creation date, publisher history, account ages) are in `table.tsv`.

Counts per target for the recommended set (C): macOS 43, Windows-gnu 42, Windows-msvc 42 (identical lists), Linux 36. Platform-only crates in C: macOS: `coreaudio-rs`, `objc2`, `objc2-audio-toolbox`, `objc2-core-audio`, `objc2-core-audio-types`, `objc2-core-foundation`, `objc2-encode`, `objc2-foundation`, `block2`, `dispatch2`, `mach2` 0.6.0; Windows: `windows` + 10 `windows-*` crates (all Microsoft / kennykerr, 2 proc-macros: `windows-implement`, `windows-interface`); Linux: `alsa`, `alsa-sys`, `pkg-config`, `libc`, `cfg-if`.

| Crate | Ver | Owners (users; teams) | Repo | build.rs | proc-macro | native C/blobs | Darwin | Win-gnu | Win-msvc | Linux | Crate vs repo | Advisories | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| alsa | 0.11.0 | diwic; team:github:rustaudio:cpal-maintainers | diwic/alsa-rs | no | no | no | - | - | - | CH | match | none | OK |
| alsa-sys | 0.4.0 | tomaka,diwic; team:github:rustaudio:cpal-maintainers | diwic/alsa-sys | yes | no | yes | - | - | - | CH | match | none | OK |
| arrayvec | 0.7.8 | bluss; team:github:rust-bus:maintainers | bluss/arrayvec | no | no | no | H | H | H | H | match | none | OK |
| atomic-arena | 0.1.2 | tesselode | tesselode/atomic-arena | no | no | no | H | H | H | H | match (CRLF/README only) | none | OK |
| audioadapter | 5.0.0 | HEnquist | HEnquist/audioadapter-rs | no | no | no | C | C | C | C | match | none | OK |
| audioadapter-buffers | 5.2.0 | HEnquist | HEnquist/audioadapter-buffers-rs | no | no | no | C | C | C | C | match (via audioadapter-rs; declared URL dead) | none | OK |
| audioadapter-sample | 5.2.0 | HEnquist | HEnquist/audioadapter-sample-rs | no | no | no | C | C | C | C | match (via audioadapter-rs; declared URL dead) | none | OK |
| audio-codec-algorithms | 0.8.1 | karip | karip/audio-codec-algorithms | no | no | no | C | C | C | C | match | none | OK |
| audio_thread_priority | 0.35.1 | padenot,kinetiknz,ChunMinChang | mozilla/audio_thread_priority | no | no | yes | - | H | H | H | match | none | OK |
| autocfg | 1.5.1 | cuviper,matklad | cuviper/autocfg | no | no | no | CH | CH | CH | CH | match | none | OK |
| bitflags | 1.3.2 | KodrAus | bitflags/bitflags | no | no | no | H | H | H | H | match | none | OK |
| bitflags | 2.13.2 | KodrAus | bitflags/bitflags | no | no | no | CH | CH | CH | CH | match | none | OK |
| block2 | 0.6.2 | simlay,madsmtm | madsmtm/objc2 | no | no | no | CH | - | - | - | match | none | OK |
| bytemuck | 1.25.2 | Lokathor; team:github:rust-bus:maintainers | Lokathor/bytemuck | no | no | no | CH | CH | CH | CH | match | none | OK |
| cc | 1.11.3 | rust-lang-owner | rust-lang/cc-rs | no | no | yes | M | M | M | M | match | none | OK |
| cfg-if | 1.0.5 | rust-lang-owner | rust-lang/cfg-if | no | no | no | H | H | H | CH | match | none | OK |
| chacha20 | 0.10.2 | newpavlov; team:github:rustcrypto:stream-ciphers | RustCrypto/stream-ciphers | no | no | no | H | H | H | H | match | RUSTSEC-2019-0029 (patched) | OK |
| coreaudio-rs | 0.14.2 | mitchmindtree,yupferris,JoshuaBatty; team:github:rustaudio:admins,github:rustaudio:coreaudio-maintainers | RustAudio/coreaudio-rs.git | no | no | no | CH | - | - | - | match | none | OK |
| core_detect | 1.0.0 | thomcc | thomcc/core_detect | no | no | no | - | H | H | H | match | none | OK |
| cpal | 0.18.2 | tomaka; team:github:rustaudio:cpal-maintainers | RustAudio/cpal | yes | no | no | CH | CH | CH | CH | match | none | OK |
| cpufeatures | 0.3.1 | tarcieri; team:github:rustcrypto:utils | RustCrypto/utils | no | no | no | - | H | H | H | match | none | OK |
| crossbeam-utils | 0.8.23 | jeehoonkang,Amanieu,taiki-e; team:github:crossbeam-rs:admins | crossbeam-rs/crossbeam | yes | no | no | H | H | H | H | match | RUSTSEC-2022-0041 (patched) | OK |
| dasp_sample | 0.11.0 | mitchmindtree | rustaudio/sample.git | no | no | no | CH | CH | CH | CH | match (subdir) | none | OK |
| dbus | 0.6.5 | diwic | diwic/dbus-rs | no | no | no | - | - | - | H | match (subdir) | none | OK |
| dispatch2 | 0.3.1 | simlay,marysaka,madsmtm | madsmtm/objc2 | no | no | no | CH | - | - | - | match | none | OK |
| encoding_rs | 0.8.44 | hsivonen | hsivonen/encoding_rs | yes | no | no | H | H | H | H | match | none | OK |
| extended | 0.1.0 | depp | depp/extended-rs | no | no | no | CH | CH | CH | CH | match | none | OK |
| find-msvc-tools | 0.1.15 | rust-lang-owner | rust-lang/cc-rs | no | no | no | M | M | M | M | match | none | OK |
| getrandom | 0.4.3 | dhardy; team:github:rust-random:maintainers | rust-random/getrandom | yes | no | no | H | H | H | H | match | none | OK |
| glam | 0.34.3 | bitshifter | bitshifter/glam-rs | no | no | no | H | H | H | H | match | none | OK |
| hound | 3.5.1 | ruuda | ruuda/hound | no | no | no | M | M | M | M | match | none | OK |
| kira | 0.12.6 | tesselode | tesselode/kira | no | no | no | H | H | H | H | match (CRLF/README only) | none | OK |
| lazy_static | 1.5.1 | rust-lang-owner | rust-lang-nursery/lazy-static.rs | no | no | no | CH | CH | CH | CH | match | none | OK |
| libc | 0.2.191 | rust-lang-owner | rust-lang/libc | yes | no | no | CMH | - | - | CMH | match (repo has extra dev-only files; Cargo.lock differs) | none | OK |
| libdbus-sys | 0.2.7 | diwic | diwic/dbus-rs | yes | no | yes | - | - | - | H | match + vendored dbus submodule (not byte-checked) | none | OK |
| libm | 0.2.16 | rust-lang-owner | rust-lang/compiler-builtins | yes | no | no | H | H | H | H | match | none | OK |
| log | 0.4.35 | rust-lang-owner | rust-lang/log | no | no | no | CH | CH | CH | CH | match | none | OK |
| mach2 | 0.4.3 | JohnTitor | JohnTitor/mach2 | no | no | no | M | - | - | - | match | none | OK |
| mach2 | 0.6.0 | JohnTitor | JohnTitor/mach2 | no | no | no | CH | - | - | - | match (repo has extra dev-only files; Cargo.lock differs) | none | OK |
| minimp3 | 0.6.1 | germangb,phip1611 | germangb/minimp3-rs.git | no | no | no | M | M | M | M | match | none | REJECT |
| minimp3_fixed | 0.5.4 | BOB450 | BOB450/minimp3-rs.git | no | no | no | M | M | M | M | match (CRLF/README only) | none | REJECT |
| minimp3-sys | 0.3.2 | germangb,phip1611 | germangb/minimp3-rs.git | yes | no | yes | M | M | M | M | match; vendored C == lieff/minimp3 @ca39bce | none | CAUTION |
| minimp3-sys | 0.6.0 | germangb,phip1611 | germangb/minimp3-rs.git | yes | no | yes | M | M | M | M | match; vendored C == lieff/minimp3 @afb604c | none | CAUTION |
| mint | 0.5.9 | kvark | kvark/mint | no | no | no | H | H | H | H | match | none | OK |
| multiversion_no_op | 1.0.0 | hsivonen | hsivonen/multiversion_no_op | no | yes | no | H | H | H | H | match | none | CAUTION |
| num-bigint | 0.4.8 | cuviper | rust-num/num-bigint | no | no | no | H | H | H | H | match | none | OK |
| num-complex | 0.4.6 | cuviper | rust-num/num-complex | no | no | no | CH | CH | CH | CH | match | none | OK |
| num-integer | 0.1.47 | cuviper | rust-num/num-integer | no | no | no | CH | CH | CH | CH | match | none | OK |
| num-rational | 0.4.2 | cuviper | rust-num/num-rational | no | no | no | H | H | H | H | match | none | OK |
| num-traits | 0.2.19 | cuviper | rust-num/num-traits | yes | no | no | CH | CH | CH | CH | match | none | OK |
| objc2 | 0.6.5 | simlay,madsmtm | madsmtm/objc2 | yes | no | no | CH | - | - | - | match | none | OK |
| objc2-audio-toolbox | 0.3.2 | simlay,madsmtm | madsmtm/objc2 | no | no | no | CH | - | - | - | match | none | OK |
| objc2-core-audio | 0.3.2 | simlay,madsmtm | madsmtm/objc2 | no | no | no | CH | - | - | - | match | none | OK |
| objc2-core-audio-types | 0.3.2 | simlay,madsmtm | madsmtm/objc2 | no | no | no | CH | - | - | - | match | none | OK |
| objc2-core-foundation | 0.3.2 | simlay,madsmtm | madsmtm/objc2 | no | no | no | CH | - | - | - | match | none | OK |
| objc2-encode | 4.1.0 | simlay,madsmtm | madsmtm/objc2 | no | no | no | CH | - | - | - | match | none | OK |
| objc2-foundation | 0.3.2 | simlay,madsmtm | madsmtm/objc2 | no | no | no | CH | - | - | - | match | none | OK |
| pastey | 0.2.3 | AS1100K | as1100k/pastey | no | yes | no | H | H | H | H | match | none | CAUTION |
| pkg-config | 0.3.34 | rust-lang-owner | rust-lang/pkg-config-rs | no | no | no | - | - | - | CH | match | none | OK |
| proc-macro2 | 1.0.107 | dtolnay | dtolnay/proc-macro2 | yes | no | no | CMH | CMH | CMH | CMH | match | none | OK |
| quote | 1.0.47 | dtolnay | dtolnay/quote | yes | no | no | CMH | CMH | CMH | CMH | match | none | OK |
| rand | 0.10.4 | dhardy; team:github:rust-random:maintainers | rust-random/rand | no | no | no | H | H | H | H | match | RUSTSEC-2026-0097 (patched in resolved) | OK |
| rand_core | 0.10.1 | dhardy; team:github:rust-random:maintainers | rust-lang-nursery/rand | no | no | no | H | H | H | H | match | old RUSTSEC (patched) | OK |
| rand_distr | 0.6.0 | dhardy; team:github:rust-random:maintainers | rust-random/rand_distr | no | no | no | H | H | H | H | match | none | OK |
| regex-lite | 0.1.9 | rust-lang-owner | rust-lang/regex | no | no | no | CH | CH | CH | CH | match | none | OK |
| rodio | 0.23.0 | tomaka; team:github:rustaudio:rodio-maintainers | RustAudio/rodio | no | no | no | H | H | H | H | match | none | OK |
| rtrb | 0.3.6 | mgeier | mgeier/rtrb | no | no | no | H | H | H | H | match | RUSTSEC-2026-0274 (patched in resolved) | OK |
| rtrb | 0.4.1 | mgeier | mgeier/rtrb | no | no | no | H | H | H | H | match | RUSTSEC-2026-0274 (patched in resolved) | OK |
| rubato | 5.1.0 | HEnquist | HEnquist/rubato | no | no | no | C | C | C | C | match | none | OK |
| rustversion | 1.0.23 | dtolnay | dtolnay/rustversion | yes | yes | no | H | H | H | H | match | none | OK |
| scopeguard | 1.2.0 | bluss,Amanieu | bluss/scopeguard | no | no | no | H | H | H | H | match | none | OK |
| shlex | 2.0.1 | comex,fenhl | comex/rust-shlex | no | no | no | M | M | M | M | match | RUSTSEC-2024-0006 (patched) | OK |
| simdutf8 | 0.1.5 | hkratz | rusticstuff/simdutf8 | no | no | no | H | H | H | H | match | none | OK |
| slice-ring-buffer | 0.3.4 | LiquidityC | liquidityc/slice_ring_buffer | no | no | no | M | M | M | M | match | **RUSTSEC-2025-0044 (unpatched)** | REJECT |
| smallvec | 1.16.6 | SimonSapin,Ms2ger,alejandro-vaz; team:github:servo:cargo-publish | smallvec/smallvec | no | no | no | CH | CH | CH | CH | match | old RUSTSEC (patched); GHSA-826h/-rpjf/-59p2/-5h7v not affecting 1.16.6 | CAUTION |
| symphonia | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia | 0.6.1 | pdeljanov | pdeljanov/Symphonia | no | no | no | CH | CH | CH | CH | match | none | OK |
| symphonia-bundle-flac | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-bundle-flac | 0.6.1 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-bundle-mp3 | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-bundle-mp3 | 0.6.1 | pdeljanov | pdeljanov/Symphonia | no | no | no | CH | CH | CH | CH | match | none | OK |
| symphonia-codec-aac | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-codec-pcm | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-codec-pcm | 0.6.1 | pdeljanov | pdeljanov/Symphonia | no | no | no | CH | CH | CH | CH | match | none | OK |
| symphonia-codec-vorbis | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-codec-vorbis | 0.6.1 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-common | 0.6.1 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-core | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-core | 0.6.1 | pdeljanov | pdeljanov/Symphonia | no | no | no | CH | CH | CH | CH | match | none | OK |
| symphonia-format-isomp4 | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-format-ogg | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-format-ogg | 0.6.1 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-format-riff | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-format-riff | 0.6.1 | pdeljanov | pdeljanov/Symphonia | no | no | no | CH | CH | CH | CH | match | none | OK |
| symphonia-metadata | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| symphonia-metadata | 0.6.1 | pdeljanov | pdeljanov/Symphonia | no | no | no | CH | CH | CH | CH | match | none | OK |
| symphonia-utils-xiph | 0.5.5 | pdeljanov | pdeljanov/Symphonia | no | no | no | H | H | H | H | match | none | OK |
| syn | 2.0.119 | dtolnay | dtolnay/syn | no | no | no | CM | CMH | CMH | CM | match | none | OK |
| syn | 3.0.10 | dtolnay | dtolnay/syn | no | no | no | MH | MH | MH | MH | match | none | OK |
| thiserror | 1.0.69 | dtolnay | dtolnay/thiserror | yes | no | no | M | M | M | M | match | none | OK |
| thiserror | 2.0.21 | dtolnay | dtolnay/thiserror | yes | no | no | MH | MH | MH | MH | match | none | OK |
| thiserror-impl | 1.0.69 | dtolnay | dtolnay/thiserror | no | yes | no | M | M | M | M | match | none | OK |
| thiserror-impl | 2.0.21 | dtolnay | dtolnay/thiserror | no | yes | no | MH | MH | MH | MH | match | none | OK |
| triple_buffer | 9.0.1 | HadrienG2 | HadrienG2/triple-buffer | no | no | no | H | H | H | H | match | none | OK |
| unicode-ident | 1.0.27 | dtolnay | dtolnay/unicode-ident | no | no | no | CMH | CMH | CMH | CMH | match | none | OK |
| visibility | 0.1.1 | danielhenrymantilla | danielhenrymantilla/visibility.rs | no | yes | no | C | C | C | C | match | none | OK |
| winapi | 0.3.9 | retep998 | retep998/winapi-rs | yes | no | no | - | M | M | - | match | none | OK |
| winapi-x86_64-pc-windows-gnu | 0.4.0 | retep998 | retep998/winapi-rs | yes | no | yes | - | M | - | - | NOT verifiable (no VCS info) | none | CAUTION |
| windowfunctions | 0.1.1 | HEnquist | HEnquist/windowfunctions-rs | no | no | no | C | C | C | C | match | none | OK |
| windows | 0.62.2 | kennykerr | microsoft/windows-rs | no | no | no | - | CH | CH | - | match | RUSTSEC-2022-0008 (patched) | OK |
| windows-collections | 0.3.2 | kennykerr | microsoft/windows-rs | no | no | no | - | CH | CH | - | match | none | OK |
| windows-core | 0.62.2 | kennykerr | microsoft/windows-rs | no | no | no | - | CH | CH | - | match | none | OK |
| windows-future | 0.3.2 | kennykerr | microsoft/windows-rs | no | no | no | - | CH | CH | - | match | none | OK |
| windows-implement | 0.60.2 | kennykerr | microsoft/windows-rs | no | yes | no | - | CH | CH | - | match | none | OK |
| windows-interface | 0.59.3 | kennykerr | microsoft/windows-rs | no | yes | no | - | CH | CH | - | match | none | OK |
| windows-link | 0.2.1 | kennykerr | microsoft/windows-rs | no | no | no | - | CH | CH | - | match | none | OK |
| windows-numerics | 0.3.1 | kennykerr | microsoft/windows-rs | no | no | no | - | CH | CH | - | match | none | OK |
| windows-result | 0.4.1 | kennykerr | microsoft/windows-rs | no | no | no | - | CH | CH | - | match | none | OK |
| windows-strings | 0.5.1 | kennykerr | microsoft/windows-rs | no | no | no | - | CH | CH | - | match | none | OK |
| windows-sys | 0.61.2 | kennykerr | microsoft/windows-rs | no | no | no | - | H | H | - | match | none | OK |
| windows-threading | 0.2.1 | kennykerr | microsoft/windows-rs | no | no | no | - | CH | CH | - | match | none | OK |


`symphonia-codec-adpcm` 0.6.1 (optional, not in the table above): owner pdeljanov; published 2026-08-13; checksum OK; identical to repo commit `ee35874` (subpath `symphonia-codec-adpcm`) and manifest identical; no `build.rs`; deps `log`, `symphonia-core` (already in the tree); verdict OK.

## 6. Open questions

1. **Bit-exactness.** `symphonia`'s MP3 decoder is a floating-point pure-Rust decoder; it is not guaranteed to match the original engine's decoder sample for sample. If "exact" includes decoded MP3 samples, which reference decoder and tolerance? (If bit-exact is required, none of the candidates here, including `minimp3`, is a safe assumption; the original decoder would have to be identified first.)
2. **Resampler exactness.** What does the original engine do (nearest, linear, sinc, none, fixed ratio)? `rubato`'s `Async` resampler offers sinc and polynomial (including linear/nearest) modes; if the original used something simpler, a small hand-written resampler may be the better fit than a high-quality one.
3. **WAV flavours.** Are any game WAVs IMA/MS-ADPCM or A-law/mu-law? ADPCM needs `symphonia`'s `adpcm` feature (vetted above); A-law/mu-law are in `pcm`. 8-bit unsigned PCM is handled by `pcm`.
4. **Linux output path.** ALSA through `cpal` (works with PipeWire's ALSA plugin) keeps the tree small. Native PipeWire support (`cpal/pipewire`) adds about 45 unvetted crates incl. `bindgen`/`clang-sys` and a libclang build dependency. Is ALSA enough? The `jack`/`pulseaudio` features were not examined.
5. **Real-time thread priority.** Do you want `cpal`'s `realtime` feature? It adds `audio_thread_priority` (Mozilla) and, on Windows, `windows-sys` (Microsoft); on Linux only a DBus-free path unless `realtime-dbus` is added (that would add `dbus` + `libdbus-sys`, system libdbus via pkg-config).
6. **`smallvec` policy.** Accept 1.16.6 as resolved, or pin 1.15.2 (my suggestion), or ask upstream `symphonia` to drop the dependency? Also: should `Cargo.lock` plus a periodic `cargo audit` + GHSA check be the standing process?
7. **Windows toolchain.** For `x86_64-pc-windows-gnu`, `windows-link`/`windows-*` use raw-dylib import libraries and need a MinGW `dlltool` at build time; not examined as a supply-chain matter, but it is another tool in the chain. Under Wine, cpal's WASAPI backend should be tested separately (outside the scope of this review).
8. **Trust anchors.** All three core crates plus `windowfunctions`/`audioadapter*` are single-person-owned (`symphonia`: pdeljanov; `rubato` family: HEnquist). The review can show consistent history and clean artifacts, not that those accounts cannot be compromised. If that residual risk matters, vendoring the exact reviewed sources (`cargo vendor` + a recorded checksum list) is the practical mitigation; the SHA-256 of every reviewed `.crate` is in `dl/SHA256.log`.

## 7. Where things are (all under /tmp/c3r-crate-vetting/)

| Path | Content |
|---|---|
| `core/`, `cmp/`, `high/` | scratch Cargo projects (recommended set / `hound`+`minimp3*` / `rodio`+`kira`) with `Cargo.lock` |
| `trees/` | `cargo tree` output per set and target |
| `membership.txt` | crate, version, set, target (which tree contains what) |
| `audit/` | `cargo audit` text + JSON per lockfile |
| `meta/`, `table.tsv` | crates.io API JSON and the flattened per-crate table (owners, downloads, publisher history, checksum check) |
| `dl/`, `src/` | downloaded `.crate` files (+ `SHA256.log`) and their unpacked, never-built sources |
| `repos/`, `export/`, `compare/` | shallow repo fetches, exported subpaths, diff results (`summary.tsv`, `manifest-check.txt`, `*.diff`) |
| `gh/` | GitHub API responses used (smallvec advisories/commits, repo health) |
| `bin/` | the scripts used, plus `verdicts.tsv` (per-crate notes behind the table) |
| `scratch-old/`, `scratch-latest/`, `cpal-*/` | early resolution attempt with older versions (not used for any conclusion) and feature-measurement projects for `cpal` |

## Answers from the project (2026-11-22)

- **WAV flavours.** Scanned all 22,995 `.wav` files in the install. The base game and its expansions are all plain PCM (format 1): 22,882 mono, 100 stereo. The only exceptions are in the third-party CCM2 scenario: 12 MS-ADPCM files (format 2, its camel units) and one MP3-in-WAV (format 0x55, `Art/Units/Elvis/(Blue Suede Shoes).wav`). So: `symphonia` features `wav`, `pcm`, `mp3` and `adpcm`. Whether symphonia's RIFF reader takes the 0x55 file is checked when the decoder is wired up. It is one mod file, so it is not a reason for another crate.
- **Exactness and MP3.** "Exact" means what the game controls: which sound plays when, its volume, pan, looping and mixing. Two MP3 decoders differ by rounding far below audibility. Music output is compared with Miles' within a tolerance, not sample for sample.
- **Resampler.** Decided once Miles' mixing rule is decoded (`docs/audio-engine.md`). If it is a simple interpolation, a few lines matching it replace `rubato`.
- **Linux.** ALSA is enough: PipeWire and PulseAudio provide ALSA compatibility, so the PipeWire crates (and `bindgen`/libclang) stay out.
- **`realtime` feature.** Not now.
- **`smallvec`.** Pinned to 1.15.2 in `Cargo.lock` when symphonia is added, as recommended.

Adopted set: `symphonia =0.6.1` (`default-features = false`, features `wav`, `pcm`, `mp3`, `adpcm`), `cpal =0.18.2` (`default-features = false`), `rubato =5.1.0` (`default-features = false`) unless the resampler point above removes it.
