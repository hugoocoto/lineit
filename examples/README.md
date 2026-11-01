# Examples

Canvases to open in the editor. Run `make` first, then:

    ./lineit -f examples/hello.lua

and press **Run** in the bar. `io/print` writes to the terminal and also shows
its last value on its square. Every file starts with a comment that explains
how it works.

**Compile** makes a program of the canvas instead, `build/bin/hello` for
`hello.lua`, that does the same without the editor:

    ./build/bin/hello

Or without the editor: `./lineit --compile examples/hello.lua`.

| File              | Shows                                                                 |
|-------------------|-----------------------------------------------------------------------|
| `hello.lua`       | `flow/entry`, `io/print` and a string constant                        |
| `fizzbuzz.lua`    | `flow/loop`, chained `flow/ifelse`, `math/modulo` and `compare/equal` |
| `clock.lua`       | `flow/clock`, resetting it from `flow/entry` and from a comparison    |
| `sum_squares.lua` | a variable (`var/var`, `var/get`, `var/set`) updated in a `flow/loop` |
| `collatz.lua`     | `flow/while` with a comparison as its condition, plus `flow/ifelse`   |
| `fibonacci.lua`   | a list variable: `list/push`, `list/get` from the end, `list/length`  |

Ctrl+S saves back to the file you opened, so copy one before experimenting
if you want to keep the original.
