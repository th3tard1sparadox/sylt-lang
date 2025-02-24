# Sylt-lang

![The Sylt mascot](res/sylt.png)

[![codecov](https://codecov.io/gh/FredTheDino/sylt-lang/branch/main/graph/badge.svg?token=8NDZVU9NPN)](https://codecov.io/gh/FredTheDino/sylt-lang)

Sylt is a statically checked and dynamically typed reference counted programming
language made for game jams.

## Why does this exist? Why use this instead of language X?

Pfft! We objectively have the best logo.

## Getting started

Sylt is written entirely in Rust. There are two main ways of using it.

### New repository

1. `$ cargo new <game name>`
2. Depend on the latest version of the `sylt`-crate with the `lingon`-feature enabled.
```toml
sylt = { version = "x.y.z", features = ["lingon"] }
```
3. Add something like this to your `src/main.rs`:
```rust
use std::path::Path;

fn main() {
    let args = sylt::Args {
        args: vec!["game.sy".to_string()],

        ..sylt::Args::default()
    };

    if let Err(errs) = sylt::run_file(&args, sylt::lib_bindings()) {
        for e in errs.iter() {
            eprintln!("{}", e);
        }
    }
}
```
4. Write your game! Here's an example to get you started:
```
x := 0.0
y := 0.0

init :: fn {
    l_bind_key("w", "up")
    l_bind_key("a", "left")
    l_bind_key("s", "down")
    l_bind_key("d", "right")

    l_bind_quit("quit")
    l_bind_key("ESCAPE", "quit")
}

update :: fn delta: float -> void {
    x += (l_input_value("right") - l_input_value("left")) * delta
    y += (l_input_value("up") - l_input_value("down")) * delta
}

draw :: fn {
    rgb :: (sin(l_time()), cos(l_time()), 0.0)
    l_gfx_rect' x, y, 1.0, 1.0, rgb
}

start :: fn {
    init'
    loop {
        if l_input_down("quit") {
            break
        }
        l_update'
        update' l_delta'
        draw'
        l_render'
    }
}
```
5. `$ cargo run` to your heart's content.

### Fork

Forking Sylt and hacking away makes it easy to do changes to the language, the
standard library and the bindings to Lingon, of which the latter two are
probably more interesting.

0. Setup a fork. (Optional)
1. Clone the repository.
2. `$ cargo run <your-game.sy>`

## Basic Usage

The `-v` flag also lets you see some debug output. If you want
to debug the compiler and runtime this might be helpful.

The `-vv` flag gives even more debug output. Don't count on seeing anything
from your own program!

## Endgame

A language that has some form of static typechecking, is easy and fast to work
in. Performance should be good enough that you don't really have to worry about
it.

Dreams exist of automatically recompiling and updating the game while the game is running.
