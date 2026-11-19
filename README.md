# mherdr

A personal fork of [herdr](https://github.com/herdrdev/herdr) that aims to feel
as responsive as a plain terminal, plus a few keyboard shortcuts. For features,
install options and usage see the
[original README](https://github.com/herdrdev/herdr#readme) and
[herdr.dev/docs](https://herdr.dev/docs/).

## what's different

- **higher frame rate** — `max_fps` in `[advanced]` (default 60, range 30–240).
  Upstream is fixed at about 60 fps, which feels sluggish on 120/144 Hz screens.
- **faster rendering** — less copying and allocation per frame, wheel scrolling
  without full redraws, release builds with fat LTO.
- **keyboard navigation** — `prefix+w` spaces and `prefix+a` agents in the
  sidebar; `j`/`k` or arrows move, `a`/`w` switch sections, `enter` opens. In
  navigate mode pane focus up/down moves to `ctrl+j`/`ctrl+k`.

```toml
[advanced]
max_fps = 144   # match your display
```

## measured

Linux, 120×40, both builds without LTO, mherdr with `max_fps = 144`, median:

|                                      | herdr   | mherdr  |
| ------------------------------------ | ------- | ------- |
| switch tab until new content         | 10.4 ms | 6.7 ms  |
| mouse wheel step until frame         | 7.7 ms  | 2.5 ms  |
| frames per second while scrolling    | 58      | 125     |
| keypress until echo                  | 1.8 ms  | 1.8 ms  |

Compare two binaries yourself with `scripts/mherdr_bench.py`. Details,
methodology and trade-offs: [MHERDR.md](MHERDR.md).

## build

```bash
cargo build --release                # fat LTO: expect 20+ minutes
./target/release/herdr server stop   # restart a running server to pick it up
```

## license

Apache License 2.0, like herdr — see [LICENSE](LICENSE).
