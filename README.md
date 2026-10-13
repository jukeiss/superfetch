# superfetch

A system information tool for the terminal, in the spirit of neofetch and
fastfetch. It prints the handful of facts you actually look at, takes a few
milliseconds to do it, and lets you change every part of the output from a
config file, from flags, or from an interactive editor.

```
                     ..'         ada@macbook
                 ,xNMM.          ───────────
               .OMMMMo           OS: macOS Sequoia 15.6.1 arm64
               lMM"              Kernel: Darwin 24.6.0
     .;loddo:.  .olloddol;.      Host: MacBook Air (13-inch, M4, 2025)
   cKMMMMMMMMMMNWMMMMMMMMMM0:    Uptime: 2h 15m
 .KMMMMMMMMMMMMMMMMMMMMMMMWd.    CPU: Apple M4 (10) @ 4.46 GHz · 12%
 XMMMMMMMMMMMMMMMMMMMMMMMX.      GPU: Apple M4
;MMMMMMMMMMMMMMMMMMMMMMMM:       Memory: 9.12 GiB / 16.00 GiB ━━━━━━──── 57%
:MMMMMMMMMMMMMMMMMMMMMMMM:       Disk: 191.85 GiB / 228.27 GiB ━━━━━━━━── 84%
.MMMMMMMMMMMMMMMMMMMMMMMMX.      Network: 192.168.1.27 (Wi-Fi)
 kMMMMMMMMMMMMMMMMMMMMMMMMWd.
 'XMMMMMMMMMMMMMMMMMMMMMMMMMMk   ████████████████████████
  'XMMMMMMMMMMMMMMMMMMMMMMMMK.   ████████████████████████
    kMMMMMMMMMMMMMMMMMMMMMMd
     ;KMMMMMMMWXXWMMMMMMMk.
       "cooc*"    "*coo'"
```

- **Fast.** About 3 ms of work on an M4 MacBook Air; see [Performance](#performance)
  for numbers and how they were measured.
- **One binary, no dependencies.** Linux, macOS and Windows. Nothing to
  install alongside it, and the source uses no third-party crates: only the
  Rust standard library. The result is about 650 KB.
- **No wrapping.** Lines are cut at the edge of the terminal, and the logo
  steps aside when the window is too narrow for it.
- **Configurable down to the line.** Every label, every value format, the
  logo, the colors and the alignment live in one TOML file.

## Install

superfetch is built with Cargo (Rust 1.88 or newer). There is nothing to
download besides the source:

```sh
git clone https://github.com/jukeiss/superfetch
cd superfetch
cargo install --path .
```

To run it whenever a terminal opens, add `superfetch` as the last line of
your `~/.zshrc`, `~/.bashrc` or PowerShell profile.

## Usage

```sh
superfetch                          # use the config file as is
superfetch --color blue             # recolor the title and labels
superfetch --toggle gpu,shell       # flip modules on or off
superfetch --logo arch              # draw another logo; "none" for no logo
superfetch -m os,cpu,memory         # exactly these lines, in this order
superfetch --set bar.width=20       # override any key of the config file
superfetch --hide network --save    # make a change permanent
superfetch --config                 # open the interactive editor
```

Flags apply to a single run unless `--save` is given, which writes them to
the config file (your comments and the rest of the file are left alone).
`superfetch --help` lists every flag and `superfetch --list` every module
and logo.

### Interactive editor

`superfetch --config` opens a menu with a live preview underneath:

| Key | Action |
| --- | --- |
| `↑` `↓` or `j` `k` | move |
| `space` | switch a module on or off |
| `J` `K` or shift + arrows | move a module down or up |
| `←` `→` or `h` `l` | change a setting |
| `s` | save |
| `q` | quit |

It covers the module list, accent color, logo, logo position, label
alignment, separator, bar style and size units. Everything else is in the
file.

## Configuration

The config file is created on first run:

| Platform | Path |
| --- | --- |
| Linux, macOS | `$XDG_CONFIG_HOME/superfetch/config.toml`, usually `~/.config/superfetch/config.toml` |
| Windows | `%APPDATA%\superfetch\config.toml` |

It lists every setting at its default with a comment explaining it, so the
file is its own reference; `superfetch --default-config` prints a fresh
copy. The short version:

```toml
# Lines of output, top to bottom.
modules = ["title", "separator", "os", "cpu", "memory", "break", "colors"]

[logo]
source = "auto"        # or "none", a built-in name, or a path to a text file
position = "left"      # left, right, top

[theme]
accent = "auto"        # "auto" follows the logo; or red, 208, "#ff8800", ...
keys = "bold accent"

[layout]
key_separator = ": "
key_align = "none"     # none, left, right

[bar]
width = 10
filled = "━"
empty = "─"

[module.memory]
label = "RAM"
format = "{used} of {total} {bar}"
```

A typo in the file does not leave you without output: superfetch reports
the line and carries on with the defaults.

The file is TOML, read by superfetch's own parser. It covers what a config
file needs (tables, strings, numbers, booleans, arrays, comments) and says
so plainly if it meets something it does not: multi-line strings, inline
tables, arrays of tables and dates.

### Formats

Each module produces named fields, and its `format` decides what is shown:

| Syntax | Meaning |
| --- | --- |
| `{name}` | a field of the module |
| `{env:NAME}` | an environment variable |
| `{#style}` | switch style: `{#bold red}`, `{#accent}`, `{#muted}`; `{#}` resets |
| `{? ... }` | optional group, dropped when a field inside it has no value |
| `{{` `}}` | literal braces |

The default CPU format shows how the optional groups keep a line tidy on
machines that cannot report a clock speed:

```toml
format = "{model}{? ({cores})}{? @ {freq}}{? · {usage}}"
```

Fields that a format does not mention are never computed. Removing
`{usage}` from the CPU line, for instance, also removes the work of
measuring it.

### Modules

| Module | Default | Fields |
| --- | :---: | --- |
| `title` | ✓ | `user` `host` `fqdn` |
| `separator` | ✓ | |
| `os` | ✓ | `pretty` `name` `version` `codename` `build` `arch` |
| `kernel` | ✓ | `name` `release` |
| `host` | ✓ | `model` `vendor` |
| `uptime` | ✓ | `pretty` `days` `hours` `minutes` |
| `shell` | | `name` `version` |
| `terminal` | | `name` `version` |
| `cpu` | ✓ | `model` `cores` `freq` `usage` |
| `gpu` | ✓ | `name` |
| `memory` | ✓ | `used` `total` `free` `percent` `bar` |
| `swap` | | `used` `total` `free` `percent` `bar` |
| `disk` | ✓ | `used` `total` `free` `percent` `bar` `path` |
| `network` | ✓ | `ip` `type` `interface` |
| `break` | ✓ | |
| `colors` | ✓ | |

### Custom modules

A `[module.<name>]` table that is not a built-in defines a new module. It
can be plain text, or capture the first line a command prints as
`{output}`:

```toml
modules = ["title", "os", "editor", "song"]

[module.editor]
label = "Editor"
format = "{env:EDITOR}"

[module.song]
label = "Playing"
command = "playerctl metadata title"
```

Commands run in parallel with the other modules, but superfetch cannot be
faster than the slowest command you give it.

### Custom logos

Point `logo.source` at a text file. Inside it, `$1` to `$9` switch to the
corresponding entry of `logo.colors` and `$$` prints a dollar sign.
neofetch's `${c1}` markers work too, so its art files can be used as they
are.

```toml
[logo]
source = "~/.config/superfetch/cat.txt"
colors = ["cyan", "#ff8800"]
```

```
  $1/\_/\
 $2( o.o )
  $1> ^ <
```

## Performance

Measured on a MacBook Air (M4, macOS 15.6), 300 runs each, output to
`/dev/null`, wall-clock time including process creation:

| Command | Median |
| --- | ---: |
| `/usr/bin/true` (cost of starting any process) | 1.5 ms |
| `superfetch` | 4.7 ms |
| `fastfetch` 2.69, same eleven modules | 26.7 ms |
| `fastfetch` 2.69, its default modules | 72.5 ms |

To check on your own machine:

```sh
hyperfine -N --warmup 20 superfetch fastfetch
superfetch --timings     # per-module breakdown
```

Where the time goes, and does not go:

- **No subprocesses.** Everything is read from the kernel or from files:
  sysctl, Mach and the I/O Registry on macOS, procfs and sysfs on Linux,
  the registry and Win32 on Windows. The one exception is the opt-in
  `{version}` field of the shell module, which has to ask the shell.
- **Nothing unused is computed.** Modules that are switched off never run,
  and within a module only the fields its format mentions are fetched.
- **Slow modules run concurrently.** Anything that may take more than a
  fraction of a millisecond gets its own thread while the cheap modules run
  on the main one, so the total is close to the slowest single module.
- **One write.** The output is assembled in memory and written once.

### CPU utilisation

Utilisation cannot be read off instantly: the kernel only keeps counters,
so a reading needs two samples with time in between. Fetch tools usually
sleep for a few hundred milliseconds to get it.

superfetch stores one sample per run in its cache directory and compares
against it on the next run, so the figure is the average utilisation since
superfetch last ran, with no waiting. If the stored sample is more than
five minutes old (the first run of the day, say), the one-minute load
average relative to the core count stands in for it; on Windows, which has
no load average, the percentage is left out until the next run.

If you would rather have a fresh measurement and pay for it in start-up
time:

```toml
[module.cpu]
sample_ms = 200
```

## Hacking

```
src/
├── main.rs         entry point; the comment at the top walks through a run
├── cli.rs          argument parsing
├── config/         schema, defaults, loading and saving the config file
├── toml.rs         the TOML reader and in-place editor
├── modules/        one file per module, plus the registry (mod.rs)
├── sys/            one file per operating system; all platform code is here,
│                   including the C and Win32 declarations (c.rs, win32.rs)
├── logo/           logo parsing, the built-in table, and art/*.txt
├── template.rs     the format string language
├── theme.rs        colors resolved against the config and the logo
├── render.rs       layout of logo and text
├── tui.rs          the interactive editor
└── style.rs, text.rs, format.rs, term.rs   small helpers
```

**Adding a module** takes one file. Copy `src/modules/uptime.rs`, change the
`MODULE` constant and the `fetch` function, and add it to `MODULES` in
`src/modules/mod.rs`. The config file, `--list`, the flags and the editor
all read from that table.

**Adding a logo**: put the art in `src/logo/art/` and add a line to
`BUILTINS` in `src/logo/mod.rs`.

**Porting to another OS**: implement the functions re-exported at the top of
`src/sys/mod.rs` in a new file next to `linux.rs`.

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

### No dependencies

`Cargo.toml` has an empty `[dependencies]` section and it is meant to stay
that way. What would usually be a crate is a file here: `src/toml.rs`
instead of a TOML library, `src/sys/c.rs` and `src/sys/win32.rs` instead of
`libc` and `windows-sys`, a few lines in `src/cli.rs` instead of an argument
parser. The operating system bindings declare only the functions and
structure fields that are used; if you add one, check its layout against
the system headers for every platform it is compiled on.

## Platform notes

Development and benchmarking happen on macOS (Apple Silicon). The Linux and
Windows back ends are built, linted and tested by the CI workflow but have
seen far less real hardware; if a value is wrong or missing on yours, please
open an issue with the output of `superfetch --timings`.

- **Linux**: GPU names come from the system's `pci.ids` database (package
  `hwdata` or `pciutils`). Without it the vendor and device ID are shown.
- **Windows**: the legacy console needs Windows 10 or newer for colors.
- **Network** reports the IPv4 address of the interface that holds the
  default route. No packet is sent to find it.

## Credits

The built-in logos come from [fastfetch](https://github.com/fastfetch-cli/fastfetch)
and [neofetch](https://github.com/dylanaraps/neofetch), both MIT licensed;
see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

## License

[MIT](LICENSE)
