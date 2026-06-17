//! kamiclj — CLI front-end to the kami-clj Clojure-subset → WASM compiler.
//!
//! The compile entry the gameka kami-clj runner shells out to (gameSpec →
//! kami-clj script → this → .wasm). Pure host tool (built for the native
//! target, not wasm32).
//!
//! Usage:
//!   kamiclj [INPUT.clj] [-o OUTPUT.wasm] [--no-prelude]
//!     INPUT omitted or "-"  → read Clojure source from stdin
//!     -o omitted            → write the WASM bytes to stdout
//!     --no-prelude          → skip GAME_PRELUDE (vec3 / timer / F32 consts)
//!
//! Exit codes: 0 ok · 1 compile error · 2 input error · 3 output error.

use std::io::{Read, Write};
use std::process::exit;

fn main() {
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut prelude = true;

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" | "--output" => output = args.next(),
            "--no-prelude" => prelude = false,
            "-h" | "--help" => {
                eprintln!("usage: kamiclj [INPUT.clj] [-o OUT.wasm] [--no-prelude]");
                return;
            }
            other => input = Some(other.to_string()),
        }
    }

    let src = match input.as_deref() {
        None | Some("-") => {
            let mut s = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut s) {
                eprintln!("kamiclj: failed to read stdin: {e}");
                exit(2);
            }
            s
        }
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("kamiclj: cannot read {path}: {e}");
                exit(2);
            }
        },
    };

    let compiled = if prelude {
        kami_clj::compile_str_with_prelude(&src)
    } else {
        kami_clj::compile_str(&src)
    };
    let wasm = match compiled {
        Ok(w) => w,
        Err(e) => {
            eprintln!("kamiclj: compile error: {e}");
            exit(1);
        }
    };

    match output.as_deref() {
        None => {
            if let Err(e) = std::io::stdout().write_all(&wasm) {
                eprintln!("kamiclj: failed to write stdout: {e}");
                exit(3);
            }
        }
        Some(path) => {
            if let Err(e) = std::fs::write(path, &wasm) {
                eprintln!("kamiclj: cannot write {path}: {e}");
                exit(3);
            }
            eprintln!("kamiclj: wrote {} bytes → {path}", wasm.len());
        }
    }
}
