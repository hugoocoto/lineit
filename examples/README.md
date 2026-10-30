# Examples

Canvases to open in the editor. Run `make` first, then:

    ./lineit -f examples/hello.lua

and press **Start** in the bar. `io/print` writes to the terminal and also shows
its last value on its square. Every file starts with a comment that explains
how it works.

| File              | Shows                                                               |
|-------------------|---------------------------------------------------------------------|
| `hello.lua`       | `flow/entry`, `io/print` and a string constant                      |
| `fizzbuzz.lua`    | `flow/loop`, chained `flow/ifelse`, `math/mod` and `compare/equal`  |
| `clock.lua`       | `flow/clock`, resetting it from `flow/entry` and from a comparison  |
| `sum_squares.lua` | variables (`var/set`, `var/get`) updated inside a `flow/loop`       |
| `collatz.lua`     | `flow/while` with a comparison as its condition, plus `flow/ifelse` |
| `fibonacci.lua`   | lists: `list/push`, `list/get` with negative indices, `list/len`    |

Ctrl+S saves back to the file you opened, so copy one before experimenting
if you want to keep the original.
