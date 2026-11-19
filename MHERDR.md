# mherdr: changes, measurements, trade-offs

## changes against upstream

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
| (branch)   | `max_fps` applies for 0.5 s after input; output alone stays at 60 fps  |

The wire protocol is unchanged, but run client and server from the same build.

## measurements

`scripts/mherdr_bench.py` starts each binary in a throwaway HOME and session
inside a 120×40 pseudo terminal and times what arrives at the host side. A
frame counts as delivered when its synchronized-output end marker
(`CSI ? 2026 l`) arrives. No pixels or terminal emulator are involved, so the
numbers are herdr's own share of the latency.

Upstream `ee5bfb50` vs mherdr `d8ef0db7`, both built without LTO (the LTO
release build is faster still), mherdr with `MAX_FPS=144`:

| test     | what is timed                                    | herdr        | mherdr       |
| -------- | ------------------------------------------------ | ------------ | ------------ |
| `switch` | `ctrl+b n` until the last frame of the new tab   | 10.4 ms      | 6.7 ms       |
| `wheel`  | one wheel step until its frame (p50 / p90)       | 7.7 / 12.7 ms | 2.5 / 2.9 ms |
| `fps`    | frames per second, wheel event every 8 ms        | 58           | 125          |
| `echo`   | keypress until the shell's echo arrives          | 1.8 ms       | 1.8 ms       |
| `flood`  | `seq 1 5000000`: duration, herdr CPU             | 6.0 s, 103 % | 5.9 s, 108 % |

With the adaptive frame rate (branch `perf/adaptive-fps`, measured against
mherdr `a1da0275`, both without LTO, `MAX_FPS=144`, two rounds each):

| test     | what is measured                                 | before       | after        |
| -------- | ------------------------------------------------ | ------------ | ------------ |
| `stream` | steady output, no input: frames/s, herdr CPU     | 101 fps, 21 % | 51 fps, 16 % |
| `flood`  | `seq 1 5000000`: duration, herdr CPU             | 5.9 s, 108 % | 6.0 s, 100 % |
| `fps`    | frames per second, wheel event every 8 ms        | 125          | 125          |
| `wheel`  | one wheel step until its frame (p50)             | 2.7 ms       | 2.8 ms       |
| `switch` | `ctrl+b n` until the last frame of the new tab   | 6.7 ms       | 6.9 ms       |
| `echo`   | keypress until the shell's echo arrives (p50)    | 2.0 ms       | 2.0 ms       |

A plain bash in the same pseudo terminal echoes in about 0.4 ms. The rest of
the echo time is the client/server round trip every multiplexer has; mherdr
does not change it.

### run it yourself

```bash
# two builds side by side, e.g. upstream in a worktree
git worktree add ../herdr-upstream ee5bfb50
(cd ../herdr-upstream && cargo build --release)
cargo build --release

for t in switch wheel fps echo; do
  scripts/mherdr_bench.py $t ../herdr-upstream/target/release/herdr
  MAX_FPS=144 scripts/mherdr_bench.py $t target/release/herdr
done
```

`N=<samples>` changes the sample count. Upstream ignores `max_fps`, so
`MAX_FPS` makes no difference there. For the render pipeline alone there is
also `just bench-render-scale`.

## trade-offs

- **CPU while scrolling** — 144 fps means more frames: wheel scrolling uses
  about 40 % of one core instead of 25 %. Idle cost is unchanged. The high
  rate only applies for 0.5 s after keyboard, mouse or resize input; output
  without input (busy agents) stays at 60 fps. Use a lower `max_fps` on
  battery or slow SSH links.
- **build time** — fat LTO turns a few minutes into 20+ minutes.
- **key change** — in navigate mode `j`/`k` now move through spaces/agents;
  pane focus up/down is `ctrl+j`/`ctrl+k`.
- **upstream syncs** — the touched render and input code can conflict when
  merging new upstream releases. `retained_flush_matches_terminal_draw_into_a_fresh_backend`
  guards the ratatui flush emulation against ratatui upgrades.
