# mherdr

A personal fork of [herdr](https://github.com/herdrdev/herdr) that aims to feel
as responsive as a plain terminal, with vim-style keyboard navigation for
spaces and agents. For features, install options and usage see the
[original README](https://github.com/herdrdev/herdr#readme) and
[herdr.dev/docs](https://herdr.dev/docs/).

## what's different

- **vim navigation for spaces and agents** — `prefix+w` jumps into spaces,
  `prefix+a` into agents; `j`/`k` move, `a`/`w` switch between the two,
  `enter` opens. Pane focus up/down in navigate mode moves to
  `ctrl+j`/`ctrl+k`.
- **higher frame rate** — upstream renders at most ~60 fps, which feels
  sluggish on 120/144 Hz screens. Set your display's refresh rate in
  `~/.config/herdr/config.toml` (default 60, range 30–240):
  ```toml
  [advanced]
  max_fps = 144
  ```
- **faster rendering** — less copying and allocation per frame, wheel scrolling
  without full redraws, release builds with fat LTO.

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
cargo build --release   # fat LTO: expect 20+ minutes
```

Install as `mherdr` next to the original `herdr`:

```bash
ln -sf "$PWD/target/release/herdr" ~/.local/bin/mherdr
mherdr server stop      # after a rebuild: restart the server to pick it up
```

Both share the config and the default session. To run them at the same time,
give mherdr its own session: `mherdr --session m` (stop it with
`mherdr --session m server stop`).

## license

Apache License 2.0, like herdr — see [LICENSE](LICENSE).
