# BASELINE.md — Electron MarkText, measured

The real-figure baseline `NATIVE-REWRITE-PLAN.md` §1.1 requires before any §12.1
target can be called a pass or a fail. Produced in M3 S0, experiment E5,
against the working tree at commit `e52106fd1cdcbd33c1258b7b0cdc7013c4c5d86c`
(`marktext-monorepo@0.20.0-dev`, 2026-07-27) in the sibling clone
`C:\Dev\marktext`.

## 1. Which rung, up front

**Rung 1 — packaged build.** `dist/win-unpacked/marktext.exe`, produced by
`electron-vite build` + `electron-builder --win --x64 --publish never`: a
minified, non-source-mapped production bundle inside a real `app.asar`, the
same artifact `pnpm run build:win` is supposed to produce. All timing, RSS,
and CPU numbers below come from launching this exe directly (not through the
NSIS installer UI — `dist/win-unpacked/` is exactly what the installer would
have placed on disk, so running it directly skips installer chrome without
changing what's measured).

**This did not come from a clean `pnpm run build:win`.** That composed script
died partway through, twice, on this machine. Both failures were fixed by
running its steps individually rather than by weakening what got measured —
see §6 for exactly what was wrong and how each was resolved. The artifact this
file reports on is the same output `build:win` would have produced had those
two environment issues not been present.

## 2. §12.1 scoreboard

| Metric | Electron, measured | Rust target | Stretch |
|---|---:|---:|---:|
| Cold start → editable caret | **818 ms** (median, n=8, range 809–829 ms) | ≤ 250 ms | ≤ 100 ms |
| Warm start | **788 ms** (median, n=5, range 783–793 ms) | ≤ 120 ms | ≤ 50 ms |
| RSS, empty document | **485 MB** (median, n=3, range 481–486 MB) | ≤ 60 MB | ≤ 35 MB |
| RSS, 1 MB document | **920 MB** (median, n=3, range 917–920 MB) | ≤ 120 MB | ≤ 70 MB |
| Open 5 MB document | **> 150 s, did not finish** (n=1, 150 s deadline; a second, un-instrumented run was let go to ≈ 5 min and still hadn't finished) | ≤ 800 ms | ≤ 300 ms |
| Keystroke → glyph, p99 | **unmeasured** — see §5 | ≤ 8 ms | ≤ 4 ms |
| Idle CPU, focused | **0.04 %** (10 s window, `250kb.md` loaded) | ~0 % | 0 % |

Six of seven rows are measured; Electron misses every target it can be
compared against, several by an order of magnitude or more, and the headline
row (open 5 MB) is not a miss but an outright failure to complete. Idle CPU is
the one row where Electron already meets the target — once painted, an idle
Chromium window really does sit near 0 %.

## 3. Per-corpus-file open time and RSS

"Open" here is cold-start-with-file: process spawn (file path as argv) to
`.editor-component` ready (§4). It therefore includes the ≈ 816 ms fixed
Electron/Chromium boot cost every time — see §4 for why a cleaner isolation
wasn't attempted. The **Δ vs `empty.md`** column subtracts that fixed cost as
an approximation of the file-specific open cost, the same spirit as the
Rust-side parse/layout split in `spikes/results/e3-layout-scale.md`, though
Electron doesn't expose a parse/layout seam to split at.

| File | Size | Open time (median, n, range) | Δ vs `empty.md` |
|---|---:|---:|---:|
| `empty.md` | 0 B | 816 ms (n=3, 813–833 ms) | — |
| `10kb.md` | 10.3 KB | 846 ms (n=3, 842–852 ms) | +30 ms |
| `50-code-fences.md` | 4.2 KB | 874 ms (n=3, 871–878 ms) | +58 ms |
| `100-inline-math.md` | 9.0 KB | 947 ms (n=3, 946–964 ms) | +131 ms |
| `20-tables.md` | 6.4 KB | 1 009 ms (n=3, 1 001–1 010 ms) | +193 ms |
| `250kb.md` | 250.2 KB | 3 628 ms (n=3, 3 613–3 685 ms) | +2 811 ms |
| `1mb.md` | 1.0 MB | **43 302 ms** (n=3, 43 115–43 430 ms) | **+42 485 ms** |
| `5mb.md` | 5.0 MB | **did not open** (§6) | **> +149 000 ms** |

The growth is not linear and not the sub-linear-converging-to-147 ns/byte
curve the Rust-side `parley` layout showed in E3. Going 10 KB → 250 KB is a
25× size increase for a 94× time increase; 250 KB → 1 MB is 4× the size for
15× the time; 1 MB → 5 MB (4× again) pushes it from 43 s to "does not finish
in 150 s," i.e. accelerating, not merely proportional. Something in the
DOM/contenteditable path is paying superlinearly per byte, which is the
opposite of what a native layout engine costs (§ e3-layout-scale.md §6).

RSS, process-tree total (§4 defines the tree; "6 procs" = 5 Electron
processes + 1 shell wrapper Playwright interposes):

| Document | RSS total (median, n=3, range) | Largest process | Rest of the tree |
|---|---:|---:|---:|
| `empty.md` | 485 MB (481–486 MB) | 168 MB | 318 MB across 5 procs |
| `1mb.md` | 920 MB (917–920 MB) | **560 MB** | 361 MB across 5 procs |

The four smaller processes are flat between the two rows (37, 51, 96→99, and
a 5 MB shell wrapper) — only one process grows with document size, from 168
to 560 MB, +392 MB for one extra megabyte of markdown. That process is almost
certainly the renderer (it holds the V8 heap, the Vue tree, and the live DOM);
Windows doesn't expose Electron's own `--type=` process-role flag through
`Get-Process`, so this is inferred from the size delta, not read off a label
— stated as inference, not fact, per §5's honesty requirement. Full six-row
breakdowns (PID, name, working set, paged memory, CPU) for both documents are
in the raw JSON under `results/rss-*.json` in this experiment's scratch
directory (not committed; reproducible by rerunning the harness in §7).

## 4. Method

**Hardware.** Intel Core i5-13600K (14C/20T), 65 320 MB RAM (63.8 GB), disks
present include a Samsung SSD 990 PRO 2TB, Windows 11 Pro 10.0.26200 — the
same physical machine the Rust-side E3 spike ran on (CPU model, RAM, OS build,
and disk model all independently confirmed on this box). No cross-machine
comparison is happening here.

**Cold start → editable caret.** "Editable caret" = the DOM state the
project's own Playwright e2e suite (`test/e2e/helpers.ts::waitForEditor`)
already treats as "the editor is ready": `.editor-component` attached to the
DOM with at least one child. This is not a literal pixel-level blinking-caret
check, but it's the same criterion the project's own test suite already
relies on to proceed with input, which makes it a principled, reproducible,
code-derived definition rather than an invented one. "Cold" = a brand-new
`--user-data-dir` per launch (no prior GPU cache, code cache, or session
state); OS file-cache for the ~450 MB of installed binaries was warm for
every run in this experiment (they'd just been built/copied), which the plan
target implicitly assumes too — nobody ships a cold page-cache machine.
Timed from `_electron.launch()` issuance to the DOM condition above resolving,
via Playwright's Electron automation (`playwright@1.61.0`, the same package
the project's own e2e suite depends on).

**Warm start.** Same DOM criterion, second-and-later launch reusing a
`--user-data-dir` that was populated by one untimed warm-up launch first (GPU
shader cache, V8 code cache, and session storage all warm).

**Open 5 MB document / per-file open.** Start = process spawn with the
corpus file's absolute path as the sole extra argv (Electron opens it as the
initial document, the same mechanism the project's own e2e `launchWithDoc`
helper uses, just with an absolute rather than fixture-relative path). Stop =
the same `.editor-component` readiness condition as cold start. This
necessarily *includes* full Electron/Chromium process boot, because Electron
has no scriptable "open this file in an already-warm window" path without
stubbing `dialog.showOpenDialog` from inside the main process — that stub was
considered but not built, given the time budget; §3's Δ column is the
mitigation, not a substitute. It is an honest limitation, not a hidden one.

**Best-of-N / N.** Every timing statistic here is **N separate process
launches**, not best-of-M reps within one process — matching the Rust side's
practice of reporting *between-launch* spread rather than only the friendlier
within-process number. Cold start N=8, warm start N=5, per-file open N=3
(reduced from 8 for time budget; spreads below 2.5% at N=3 suggest this
under-samples less than the reduction implies). Every table cell states
median plus the full min–max range; no number here is a single run.

**RSS.** Process-tree sum via `Get-CimInstance Win32_Process` walked from the
launched process's own PID (recursive parent→child BFS), summing
`WorkingSet64` across every descendant — main, renderer(s), GPU, utility,
crashpad. Sampled 2 s after the editor-ready condition to let the initial
paint settle, 3 launches per document, median reported. This is exactly the
process-tree requirement in the assignment: a main-process-only number would
have read as 96–168 MB and completely hidden the 485–920 MB reality.

**Idle CPU, focused.** `250kb.md` loaded and ready, window brought to front,
2 s settle, then `TotalProcessorTime` sampled at the start and end of a 10 s
window across the whole process tree; delta CPU-seconds ÷ (window seconds ×
20 logical cores) × 100. One run — the number was small and stable enough
(0.04 %) that repetition wasn't going to change the conclusion, and CPU
budget was better spent on §3/§6.

**Cleanup discipline.** Every launch used a fresh temp `--user-data-dir`,
removed after the process was force-killed (`taskkill /T /F` on the root PID)
regardless of success or failure, so no measurement run could be warmed by a
previous one's leftover state, and no orphaned `marktext.exe` could linger
into the next measurement and pollute idle-CPU or RSS numbers. This was worth
stating explicitly because the very first launch of this experiment collided
with **pre-existing, already-running `marktext.exe` v0.19.1 processes** on
this machine — an installed release, unrelated to this experiment, with
`%APPDATA%\marktext\logs` history stretching back to July 2025 — that were
holding Electron's single-instance lock. Those were stopped
(`Stop-Process -Force`) to unblock testing; they showed no activity for the
prior several days at the time and are very likely disposable background
processes rather than a session with unsaved work, but this is a real side
effect of this experiment that a person should know about, not something to
bury.

## 5. What this file does not measure

- **Keystroke → glyph, p99 — left unmeasured, not estimated.** Playwright can
  dispatch synthetic key events, but a synthetic keystroke doesn't traverse
  the same input path as real hardware (no HID → OS input-queue → Chromium
  compositor round trip), and this experiment had no way to hook Chromium's
  actual frame-presentation timestamp (would need CDP `Page` domain tracing
  or an external high-speed capture, neither built here). A DOM-mutation
  timestamp was considered as a proxy and rejected — it measures "the model
  updated," not "a glyph was painted," and reporting it as the latter would
  be exactly the guessed number this file is supposed to avoid. Left blank
  per the assignment's explicit instruction that an honestly missing row
  beats a guessed one.
- **Installer size / installed-on-disk size** — not a §12.1 row, but since
  the artifacts exist: NSIS installer `marktext-win-x64-0.20.0-dev-setup.exe`
  is 119.3 MB; `dist/win-unpacked/` on disk is 451.8 MB. Offered as bonus
  context for §1.1's fuller table, not folded into the §12.1 scorecard above.
- **macOS / Linux.** Windows x64 only, matching §12.1's own scope.
- **A truly cold OS file cache.** Every binary and bundle involved had just
  been built or copied in this same session, so Windows' file-system cache
  was warm for literally every run, cold-start included. Flushing it would
  need a privileged cache-clearing tool (e.g. RAMMap) not exercised here.
  This very likely does not matter much for Electron specifically — its
  dominant cost by far is V8/bundle *execution*, not disk I/O (see the flat
  ~800 ms cold vs. ~790 ms warm gap in §2) — but it is a limitation of every
  number in this file, stated once here rather than caveatted per row.
- **Multiple simultaneous windows/tabs, or a long-running session's RSS
  growth over hours.** Every RSS/CPU sample is a fresh, short-lived process.
- **The dev-mode / rung-3 numbers.** Rung 1 was reached, so rungs 2–3 weren't
  needed for the scoreboard. One rung-2 spot check (`electron.exe` running
  the already-built `out/` directly, no packaging) did confirm the app
  reaches a real, responsive `MarkText`-titled window — used only to
  de-risk rung 1's own debugging, not reported as a number, since a
  half-run secondary rung is worse noise than no rung 2 at all.

## 6. `5mb.md`: exactly how it fails, and two build blockers this experiment had to clear

### 6.1 The failure itself

`5mb.md` does not open. Two runs:

**Instrumented run (150 s deadline, harness-sampled every ~15 s):**

| t | process-tree RSS | cumulative CPU (6 procs) |
|---:|---:|---:|
| 0–93 s | (process not yet spawned in the tree walk — see below) | — |
| 108 s | 808 MB | 128.9 s |
| 124 s | 798 MB | 146.7 s |
| 139 s | 701 MB | 165.0 s |
| 155 s | **970 MB** | **182.8 s** |

At 150 s the harness's own readiness wait (`.editor-component` ready) timed
out; the process was still running, still `Responding = True` (Windows'
message pump was not blocked — this is not a classic UI-thread deadlock), and
still consuming CPU. RSS is not monotonically climbing (701→970 MB, one GC
pass visible at 139 s) — this is real, sustained computation, not a stuck
allocator.

**A second, earlier, un-instrumented run** (same build, same file, no
argument-order fix yet applied to the harness) was watched manually via
`Get-Process` rather than the harness and let run to **≈ 5 minutes** before
being force-killed: cumulative CPU on the largest process alone had reached
**≈ 306 s** (i.e., that one process alone had used more compute-seconds than
had elapsed wall-clock — consistent with multiple threads, not one pegged
core), working set fluctuating 430–777 MB, `Responding = True` throughout.
Killed by the harness/operator, not by the app recovering or crashing on its
own — MarkText was never observed to give up by itself.

**Reading this against the plan.** NATIVE-REWRITE-PLAN.md's own pre-measurement
estimate for this row was "3–15 s (often unusable)". The real number is
worse by well over an order of magnitude — not slow-but-usable, and not a
clean multi-second failure either, but multiple *minutes* of a process that
looks alive (responding, computing, not crashing) while producing nothing a
user could interact with. "Electron cannot do this at all" — the claim
M3-R3 exists to test — is confirmed, and confirmed harder than the plan
guessed.

### 6.2 Two build blockers, and how each was resolved

Both are disclosed here because both required deviating from the literal
`pnpm install && pnpm run build:win` sequence in §1.1, and a baseline that
doesn't say what it deviated from and why is not trustworthy.

1. **`electron-rebuild -f` (part of `build:win`) fails on `native-keymap`.**
   `pnpm install`'s postinstall step failed with `MSB8040: Spectre-mitigated
   libraries are required for this project`, a Visual Studio component
   (`Microsoft.VisualStudio.Component.VC.14.44.17.14.x86.x64.Spectre`) not
   installed on this machine; two attempts to install it via `vs_installer.exe
   modify` both failed (exit 87, invalid parameter, root cause not chased
   further — time-boxed). **Resolution:** ran
   `electron-rebuild --only=ced,keytar` (native-keymap excluded) instead of
   the blanket `-f`. `ced` (encoding detection) and `keytar` (credential
   store) rebuilt cleanly for Electron's ABI. `native-keymap` (OS
   keyboard-layout enumeration — irrelevant to opening/rendering markdown)
   was left with no compiled binary at all; empirically this does **not**
   crash the app (confirmed by a clean rung-2 boot with no native-keymap
   binary present), so its absence does not compromise anything this file
   measures.
2. **`electron-builder`, run directly rather than via a `pnpm`-invoked
   script, mis-detected the package manager and silently dropped
   `@vscode/ripgrep-win32-x64`** from the packaged `app.asar` — a real, latent
   bug in this monorepo's build path, not an artifact of this experiment's
   workarounds. The first packaged build crashed on launch with `Could not
   find @vscode/ripgrep-win32-x64. Ensure optionalDependencies are installed
   for this platform`, caught via a real `dialog.showErrorBox` window read
   through Windows UI Automation. **Resolution:** reran packaging as
   `pnpm exec electron-builder --win --x64 --publish never` (rather than
   calling the `.bin` directly) so `npm_config_user_agent` correctly signalled
   pnpm; electron-builder then found and unpacked `rg.exe` into
   `app.asar.unpacked` as intended. All numbers in this file come from the
   corrected build. This bug would affect anyone packaging this monorepo
   without going through a `pnpm run`/`pnpm exec` wrapper and is worth a
   maintainer's attention independent of this experiment.

Neither workaround touched `src/`, `bench/corpus/`, or any committed file in
either repository; both are build-invocation choices, reproducible by anyone
hitting the same two environment issues.

## 7. Reproducing

```sh
# in C:\Dev\marktext
pnpm install                                            # postinstall may fail on native-keymap; see §6.2.1
cd packages/desktop
pnpm exec tsx ../../scripts/minify-locales.ts
pnpm exec electron-rebuild --only=ced,keytar
pnpm exec electron-vite build
pnpm exec electron-builder --win --x64 --publish never  # NOT the bare .bin — see §6.2.2
# dist/win-unpacked/marktext.exe is the artifact this file measures
```

The Playwright-driving harness used for all timing/RSS/CPU numbers here is
not part of either repository (it lived in this session's scratch directory);
it is a thin wrapper around `playwright`'s `_electron.launch()` plus a
`Get-CimInstance`-based process-tree walker, following exactly the
definitions in §4. It is not committed anywhere per this experiment's scope.
