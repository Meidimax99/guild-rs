# ESC event scripts

Reconstructed from the interpreter in `GildeGold_TL.exe` (`0x420000`–`0x424a00`).
Implementation: `crates/gilde-esc`. All 461 effective scripts (`scripts.bin`
overlaid with `scripts.bin1`) parse; semantics are covered by
`crates/gilde-esc/tests/vm.rs`.

## Engine structure

| Address | Name (from debug strings) | Role |
|---|---|---|
| `FUN_00421820` | `evt_LoadScript` | read source (≤ 64000 bytes), preprocess, allocate slot (max 128 scripts) |
| `FUN_00421590` | – | preprocess: strip `/* */`, newlines/tabs → spaces, collapse runs of spaces, build line table |
| `FUN_00422960` | – | pre-parse: collect globals and functions at brace depth 0 |
| `FUN_00421150` | – | tokenizer (operators, keywords, words, numbers, strings) |
| `FUN_00423220` | – | expression evaluator |
| `FUN_00423be0` | – | execute one statement |
| `FUN_00424090` | – | per-script tick: re-poll latent call, then step |
| `FUN_004241e0` | – | tick all scripts |
| `FUN_00423180` | – | stop script: run `exit()` once (multistep), then free |
| `FUN_004248a0` | `evt_ImportCommand` | register a host function (≤ 256, names ≤ 31 chars) |

The interpreter walks the preprocessed source text directly; there is no
bytecode.

## Language

* Types `int`, `float`, `string` (96-byte buffer), `byte`/`char` (signed 8 bit),
  `void`. Scalars are arrays of length 1; `type name[N];` declares arrays.
  Globals may be initialised with a number literal.
* Statements: declaration, `name[idx] = expr;`, `name++;`, `name--;`, calls,
  `if (…) stmt [else stmt]`, `while (…) stmt`, `return [expr];`, `{ … }`,
  `#include "file"`, `#singlestep`, `#normalstep`, `#multistep`.
* No `for`, `do`, `!`, `%`, or line comments (the shipped scripts don't use them).
* Strings are raw: no escape sequences (`"kampf\hieb"` is literal).

### Expression evaluation (`FUN_00423220`)

* Every value is a 32-bit integer; float literals are truncated, float
  variables store `(float)int`.
* `+ - * / | &` fold strictly **left to right** with no precedence.
  A leading operator applies to 0 (`-1`).
* A comparison takes the **entire rest** of the expression as its right operand:
  `a == b + 1` is `a == (b + 1)`.
* Before evaluating, the engine scans ahead for `&&`/`||` (stopping at
  `( ) ; {`); the left part is evaluated on its own and combined with the rest
  using bitwise `&`/`|` — **no short-circuit**.
* `=` inside an expression replaces the accumulator and assigns nothing.
* Strings evaluate to the single global string buffer, so comparing two strings
  is always "equal".
* Script functions can only be called as statements; the shipped scripts' helper
  functions are all `void`.

### Scheduling

* One engine tick runs one statement per script in `#normalstep` and
  `#singlestep` mode; `#multistep` runs until the mode changes or the script
  ends.
* Host calls can be **latent** (walks, animations, camera flights): they set
  the script's pending-command field and are re-invoked every tick with the same
  arguments until done. Only `#singlestep` waits for them; `Sleep` always waits.
* `Sleep(ms)` waits `ms / 14` ticks; `Sleep(-1)` waits forever (until stopped).
* When `main` returns, or the script is stopped (`StopScript`,
  `KillLocalScripts`, scene change), `exit()` runs once in multistep mode.

### Leniency

The engine tolerates malformed scripts and so does the parser: junk at file scope
is skipped, stray operators in statements are ignored, a missing `)` ends at `;`,
extra tokens before `;` are skipped, prototypes are ignored, untyped parameters
are dropped (making `RunScript` fail with "Wrong parameter count"), and an
unknown word stops the script at runtime ("Unknown symbol").

## Host API

`docs/esc_api.tsv` lists all 129 registered host functions with their argument
types (extracted by `re/scripts/esc_api.py`). The scheduler implements
`Sleep`, `RunScript*`, `StopScript`, `FindScript` and `KillLocalScripts`; the rest
are engine functions provided through `gilde_esc::Host`.
