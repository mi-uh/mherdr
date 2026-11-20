# mherdr: changes, measurements, trade-offs

## Changes against upstream

| commit     | change                                                                 |
| ---------- | ---------------------------------------------------------------------- |
| `c4ff112a` | ansi encoder skips unicode width lookups for plain ascii cells          |
| `38d62ce3` | wheel scrolling streams retained rows instead of a full UI render       |
| `dd80ffc5` | client moves patch rows instead of cloning, less tab layout work        |
| `b4ba4a8f` | release profile: fat LTO, one codegen unit (render benchmark −13.5 %)   |
| `782dc701` | keyboard navigation for spaces and agents                               |
| `8077ef0a` | `[advanced] max_fps` replaces the fixed 16 ms render interval           |
| `48a79f7c` | client sends requests to the server before composing its own frame     |
| `877a53f2` | cell symbols stored inline (CompactString) instead of one heap string  |
| `d8ef0db7` | tab surfaces rendered without a ratatui flush and extra copies          |
| `46a99640` | `max_fps` applies for 0.5 s after input; output alone stays at 60 fps  |
| `e297d1b6` | `prefix+i` jumps to the next blocked or finished agent                  |

**Compatibility:** wire protocol unchanged, but run client and server from the
same build.

## Measurements

`scripts/mherdr_bench.py` runs each binary in a throwaway HOME at 120×40 and
times bytes at the PTY (frame end = `CSI ? 2026 l`). No terminal emulator
involved.

Upstream herdr `e8d7d970` vs mherdr `4309e14b` (2026-11-20), both without LTO,
mherdr with `MAX_FPS=144`, p50, two rounds. Lower is better except fps.

| test                                  | herdr         | mherdr        | effect             |
| ------------------------------------- | ------------- | ------------- | ------------------ |
| `switch`: tab switch until new frame  | 13.5 ms       | 6.7 ms        | **2× faster**      |
| `wheel`: wheel step until its frame   | 9.1 ms        | 2.5 ms        | **3.6× faster**    |
| `fps`: scroll steps drawn (125 per s) | 58 / s        | 125 / s       | **every step, none skipped** |
| `echo`: keypress until echo           | 5.6 ms        | 2.0 ms        | **2.8× faster**    |
| `stream`: agent prints, you idle      | 52 fps, 41 % CPU | 52 fps, 16 % CPU | **−60 % CPU**  |
| `flood`: `seq 1 5000000`              | 5.8 s, 131 % CPU | 5.8 s, 102 % CPU | **−22 % CPU**  |

`fps` sends one wheel event every 8 ms, so 125/s is the maximum, not `max_fps`.

### Run it yourself

**Requires:** two release builds; run from the repo root.

```bash
git worktree add ../herdr-upstream upstream/master
(cd ../herdr-upstream && cargo build --release)
cargo build --release

for t in switch wheel fps echo stream flood; do
  MAX_FPS=144 scripts/mherdr_bench.py $t \
    ../herdr-upstream/target/release/herdr target/release/herdr
done
```

`N=<samples>` sets the sample count; upstream ignores `MAX_FPS`. Render
pipeline only: `just bench-render-scale`.

## Trade-offs

| area           | cost                                                                   |
| -------------- | ---------------------------------------------------------------------- |
| CPU            | scrolling at 144 fps: ~40 % of a core instead of 25 %; lower `max_fps` on battery or slow SSH |
| build time     | fat LTO: 20+ minutes instead of a few                                  |
| keys           | navigate mode: `j`/`k` move lists, pane up/down is `ctrl+j`/`ctrl+k`   |
| upstream syncs | render and input changes can conflict; `retained_flush_matches_terminal_draw_into_a_fresh_backend` guards the ratatui flush emulation |
