# Resume here (2026-11-26)

Handoff for continuing this fork's work on another PC. Read this first, then
[telemetry.md](telemetry.md) and the last entry of `.flowestra/decisions.md`.

## Goal

Run the runtime as fast as possible on a 32-lane NVIDIA card (RTX 5080): reach 30 fps, then go
beyond it. First step: full, machine-readable instrumentation, so a run explains itself without
anyone watching it.

## Done

- **Synced 154 upstream commits** into `dev` (merge `052fddf`).
- **Security review of those commits:** four AI reviewers read all 19,001 diff lines; findings were
  checked against the code. No malicious code found. Older code and dependencies were not reviewed.
- **Hourly sync is off.** The `fork-sync` workflow is disabled on GitHub. Syncing is manual.
- **Telemetry parts 1 to 6 built and tested** (2026-11-26): 14 commits on branch `adopt/diagnostics-2`
  (worktree `xcl-adopt`), on top of 15 adopted upstream PRs. The branch is not pushed and `dev` has not
  been moved to it: that is the user's decision. Status table: `telemetry.md` section 8. Last
  whole-suite result (RTX 5080, desktop session): 1603 passed, 4 failed
  (`gpu::dxil::fuzz_tests::fuzz_validate_and_gpu_0..3`, failing before this work), 47 ignored, 44
  SKIPPED lines. Not tested on AMD. Smoke run of the title, 60 s, telemetry off: 1,659 frames against
  1,662 on `dev`, no device removal, no crash.
- **`CLAUDE.md` hard limits** added (no game data, gated merges, commit only when asked).
- **Merge protection, first version** (`scripts/dev/pr-gate.py`, `merge-pr.py`, `merge-guard.py`,
  `.githooks/pre-merge-commit`, `test_merge_protection.py`): 60 tests passed on 2026-11-25, but
  review found gaps (below).

## Left to do, in order

1. **Finish merge protection.** Do not merge outside code until this is done. The seven fixes are in
   `.flowestra/decisions.md`, "Protection review refinements". The pushed files
   are the first version (60 tests pass) and contain none of
   them. A stopped attempt at them is not in the repository. Main ones:
   - merges must run with an empty hooks folder (an incoming `.githooks` file could run mid-merge);
   - refuse changes to the protection files unless explicitly allowed;
   - the hook must read `GITHEAD_<id>` variables (git writes `MERGE_HEAD` only after the hook);
   - ask before shell commands write review or approval records, or edit the guard;
   - cover `timeout`, PowerShell `iex` / `Start-Process`, and `patch`;
   - binary files must be named in the review.
   Then repeat an adversarial review before relying on it.
2. **Deep dive on the title's freeze past the menus**, with telemetry at tier `pass`. Known before
   the dive: frames stop about 96 to 117 s after the first button press; the stub
   `ID3D12GraphicsCommandList10::#101` is called right before; the menus are frame-clock bound (about
   33 ms per frame, of which about 2 ms CPU and under 2 ms GPU). What the title shows is labelled by
   the user from a screenshot (phase control file), not guessed.
3. **After that, by the user's choice and with measurements first:** items from the study of another
   project's AMD work (kept outside the repo). Nothing from it may be ported: its licence is
   GPL-3.0-or-later, XCL is GPL-2.0-only.
4. **Split-wave exchange report:** build it cleanly on a new
   branch, `git switch -c shaders/wave64-exchange-report origin/main` (the old one was local and
   empty), so it could be shared. Decide later whether to send it.

## Open questions for the user

1. Move `dev` to `adopt/diagnostics-2` (and push it), or not?
2. Which title, if any, will be measured? Unanswered.
3. Publish the exchange report upstream once its numbers are known?
4. Report the two test findings below upstream? Nothing has been posted.

Settled: D1 to D17 are in `telemetry.md`. Telemetry code is in the fork-only files
`crates/xcl/src/telemetry_gpu.rs` and `crates/xcl/src/telemetry_run.rs` (included from `report.rs`),
with event and manifest plumbing in `log.rs` and `report.rs` and one-line call sites in upstream files
(this replaces the earlier D6); overhead is measured (`telemetry.md` 5.10); every outside change needs a full diff review plus the user's approval; building
reviewed code locally is fine.

## Known test state

At `a67c592` on an RTX 5080, driver 621.01, Rust 1.100.0:
**1,285 passed, 5 failed, 46 ignored, 26 skip messages.**

- `a_bridged_mask_write_where_one_half_is_elsewhere_meets_no_barrier`: stale expectation. #2074's
  test met #2203's change when merged. It also fails in upstream's CI. Not run: turning
  `can_wave_branch` off to confirm. Matters only with `XCL_PER_LANE=1`.
- `fuzz_validate_and_gpu_0` to `_3` (seeds 70, 545, 1019, 1507): the NVIDIA driver refuses the
  pipeline (`0x8007000E`), alone and with `XCL_DXIL_NO_OPT=all`. Likely cause: groupshared
  subtraction, where upstream's workaround does not hold on this driver. Not memory exhaustion.

Bugs found in review, none looking deliberate, none fixed here:

| Priority | Bug | From |
|---|---|---|
| Medium | `XCL_NETWORK=0` lets `127.<anything>` names reach real DNS (`netoff.rs:53`) | older; #2312 extended it |
| Low | a huge split dispatch can exhaust memory (`pso.rs` `dispatch_slabs`) | `e9ddf17` |
| Low | sampler copies can pass the end of a short page (`console_heap.rs` `publish_range`) | #1991 |
| Low | video memory may be freed while another queue uses it (`vram.rs`); unconfirmed in practice | #1985 |
| Low | fresh translations skip the container check (`pso.rs` `cached_rerouted`) | #2120 |
| Low | shader cache entries are not checked against tampering | older; #2080 widened it |
| Low | `XMemVirtualQuery` writes 16 bytes at +48; real size unknown | #2048 |

## Facts not to get wrong

- Nothing has been measured on this hardware. Upstream's README says about 5 fps in the 3D world on
  an RTX 5090. The often-quoted 1 fps is another project on another card.
- The loop cap is 65,536, not 4 million.
- `XCL_WAVE64_SPIN=0` affects only groups of one console wave, not the multi-wave case.
- None of these has merged (checked 2026-11-25): #1193, #2343, #1596, #1597, #1111, #2377, #2097,
  #2210, #917.
- Telemetry must not reuse the timestamp pool in `list_x.rs`.

## Installed on the old PC

| Tool | Version | Installed by us |
|---|---|---|
| Rust (through rustup 1.29.3) | 1.100.0, `stable-x86_64-pc-windows-msvc` | yes, with winget |
| Gaming Services | 39.118.30001.0, gives `xgameruntime.dll` | yes: first attempted with winget, present later |
| Visual Studio C++ x64 tools | Visual Studio 18 Community | already there |
| Windows SDK / DXC | 10.0.26100.0 / 1.8.2502.11 | already there |
| Python, GitHub CLI | 3.14.2, signed in | already there |
| `core.hooksPath` | `.githooks` | yes, for that clone only |

Cargo is in `%USERPROFILE%\.cargo\bin`; Git Bash needed it added to `PATH`.

## Setting up the other PC

Windows 10 or 11 x64. These do not travel with git:

1. Visual Studio with the C++ x64 tools, the Windows SDK (DXC 1.8 or newer), Python 3.12 or newer,
   and the GitHub CLI signed in (`gh auth login`).
2. `winget install --id Rustlang.Rustup --exact`, toolchain `stable-x86_64-pc-windows-msvc`.
3. Microsoft Store: Gaming Services (gives `xgameruntime.dll`; needed only to run a title).
4. `git clone https://github.com/Osanchez/xcl.git`, then `git switch dev`.
5. `git config core.hooksPath .githooks` (set per clone).
6. `python -I scripts/dev/test_merge_protection.py`, then
   `cargo test --workspace --no-fail-fast -- --test-threads=4` (do not pipe it through `tail`: that
   hides the exit code). A different graphics card gives different skips and failures.

Review records (`.git/pr-gate/`), logs and Claude's session history and memory stay on the old PC.
Nothing needed depends on them.

To resume, tell Claude: "Read docs/guide/resume-here.md and continue with item 1."
