# Examples

Canvases to open in the editor. Run `make` first, then:

    ./lineit -f examples/hello.lua

and press **Start** in the bar. `print` writes to the terminal and also shows
its last value on its square. Every file starts with a comment that explains
how it works.

| File              | Shows                                                           |
|-------------------|-----------------------------------------------------------------|
| `hello.lua`       | `entry`, `print` and a string constant                          |
| `fizzbuzz.lua`    | `loop`, chained `ifelse`, `math/mod` and `compare/equal`        |
| `clock.lua`       | `clock`, resetting it from `entry` and from a comparison        |
| `sum_squares.lua` | variables (`var/set`, `var/get`) updated inside a `loop`        |
| `collatz.lua`     | `while` with a comparison as its condition, plus `ifelse`       |
| `fibonacci.lua`   | lists: `list/push`, `list/get` with negative indices, `list/len`|

Ctrl+S saves back to the file you opened, so copy one before experimenting
if you want to keep the original.
