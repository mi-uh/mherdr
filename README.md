# mherdr

A personal fork of [herdr](https://github.com/herdrdev/herdr) that aims to feel
as responsive as a plain terminal and extends herdr's vim-style navigation from
panes to spaces and agents. Everything else works as in herdr; more in the
[original README](https://github.com/herdrdev/herdr#readme) and
[docs](https://herdr.dev/docs/).

## what's different

- **vim navigation everywhere** — `prefix+w` jumps into the space list,
  `prefix+a` into the agent list. There `j`/`k` move, `enter` opens, and `a` or
  `w` jumps straight to the other list. Panes keep `h`/`l`; pane up/down moves
  to `ctrl+j`/`ctrl+k` because `j`/`k` now drive the lists.
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

## install

Prebuilt binaries for Linux (x86_64, aarch64, static) and macOS (x86_64,
arm64) are on the [releases page](https://github.com/mi-uh/mherdr/releases):

```bash
curl -fLo ~/.local/bin/mherdr \
  https://github.com/mi-uh/mherdr/releases/latest/download/mherdr-linux-x86_64
chmod +x ~/.local/bin/mherdr
```

macOS binaries are not signed; run `xattr -d com.apple.quarantine mherdr` once.

## build

```bash
cargo build --release   # fat LTO: expect 20+ minutes
```

Install your own build as `mherdr` next to the original `herdr`:

```bash
ln -sf "$PWD/target/release/herdr" ~/.local/bin/mherdr
mherdr server stop      # after a rebuild: restart the server to pick it up
mherdr
```

Both share the config and the default session. To run them at the same time,
give mherdr its own session: `mherdr --session m` (stop it with
`mherdr --session m server stop`).

`herdr update` is disabled in mherdr because it would replace the build with
upstream herdr. Update with `git pull` and a rebuild instead.

## license

Apache License 2.0, like herdr — see [LICENSE](LICENSE).
