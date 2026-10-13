//! ESC — the event-script language of *Die Gilde*.
//!
//! The original engine (`evt_*`, `0x420000`–`0x424a00` in `GildeGold_TL.exe`)
//! interprets preprocessed source text directly. This crate parses scripts into an
//! AST once and runs them on a resumable VM that reproduces the original semantics:
//!
//! * **Values are 32-bit integers.** Float literals are truncated on read
//!   (`FUN_00421150` stores `(int)(float)atof(tok)`), strings evaluate to the
//!   engine's global string buffer.
//! * **No operator precedence** for `+ - * / | &`: they fold strictly left to
//!   right (`FUN_00423220`), so `1 + 2 * 3` is `9`.
//! * A comparison (`== != < <= > >=`) takes the *entire rest* of the expression
//!   as its right operand.
//! * `&&` / `||` split the expression first (`FUN_004210f0`) and combine the
//!   truth values with bitwise `&` / `|` — no short-circuit evaluation.
//! * Statements: declarations, assignment, `++`/`--`, calls, `if`/`else`,
//!   `while`, `return`, blocks, `#include`, and the stepping directives
//!   `#singlestep`, `#normalstep`, `#multistep`.
//! * Scheduling (`FUN_00424090`): normal and single-step mode execute one
//!   statement per engine tick, multi-step runs until the mode changes. Host
//!   calls can be *latent* (e.g. `Sleep`, `WalkToDummyVerified`): in single-step
//!   mode the script blocks and the call is re-polled every tick until it
//!   completes; `Sleep` always blocks.

pub mod ast;
pub mod parser;
pub mod vm;

pub use ast::{Program, Type};
pub use parser::{ParseError, parse};
pub use vm::{Host, HostResult, Script, StepMode, Value};
