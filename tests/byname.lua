-- byname: a fan-out meeting itself (clear, then push a, once), and
-- variables used by their name typed in the field, without wires: one
-- declared by a var/var nothing is wired to (n, 10 at the start), others
-- by none. The two entries meet before printing. Checked by `make test`
-- (see tools/run_tests.c).
lineit_canvas = "0.0.1" -- the lineit that wrote it, see LI_VERSION

camera = { x = 0, y = 0, offset_x = 0, offset_y = 0, zoom = 1 }

squares = {
        { id = 50, module = "flow/entry", x = -35, y = -35 },
        { id = 51, module = "var/var", x = 116.898438, y = 226.898438, params = { { name = "name", value = "l" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "list" }, { name = "start", type = "int", value = "0" }, } },
        { id = 52, module = "list/clear", x = 119.6875, y = -220.3125, inputs = { { name = "list", type = "ref", value = "" }, } },
        { id = 53, module = "list/push", x = 306.898438, y = -73.1015625, inputs = { { name = "list", type = "ref", value = "" }, { name = "value", type = "string", value = "a" }, } },
        { id = 54, module = "list/push", x = 496.898438, y = -73.1015625, inputs = { { name = "list", type = "ref", value = "" }, { name = "value", type = "string", value = "b" }, } },
        { id = 55, module = "list/push", x = 686.898438, y = -73.1015625, inputs = { { name = "list", type = "ref", value = "" }, { name = "value", type = "string", value = "c" }, } },
        { id = 56, module = "var/var", x = -73.1015625, y = 676.898438, params = { { name = "name", value = "n" }, { name = "global", value = "false" }, }, inputs = { { name = "type", value = "int" }, { name = "start", type = "int", value = "10" }, } },
        { id = 57, module = "flow/entry", x = -35, y = 565 },
        { id = 58, module = "var/get", x = 119.6875, y = 679.6875, inputs = { { name = "var", type = "ref", value = "n" }, } },
        { id = 59, module = "list/push", x = 116.898438, y = 526.898438, inputs = { { name = "list", type = "ref", value = "seen" }, { name = "value", type = "int", value = "0" }, } },
        { id = 60, module = "var/set", x = 304.109375, y = 524.109375, inputs = { { name = "var", type = "ref", value = "n" }, { name = "value", type = "int", value = "5" }, } },
        { id = 61, module = "var/get", x = 499.6875, y = 679.6875, inputs = { { name = "var", type = "ref", value = "n" }, } },
        { id = 62, module = "list/push", x = 496.898438, y = 526.898438, inputs = { { name = "list", type = "ref", value = "seen" }, { name = "value", type = "int", value = "0" }, } },
        { id = 63, module = "list/push", x = 686.898438, y = 526.898438, inputs = { { name = "list", type = "ref", value = "names" }, { name = "value", type = "string", value = "x" }, } },
        { id = 64, module = "var/get", x = 974.6875, y = 229.6875, inputs = { { name = "var", type = "ref", value = "l" }, } },
        { id = 65, module = "string/join", x = 1069.6875, y = 229.6875, inputs = { { name = "sep", type = "string", value = "," }, } },
        { id = 66, module = "io/print", x = 1066.89844, y = 76.8984375, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 67, module = "var/get", x = 1164.6875, y = 229.6875, inputs = { { name = "var", type = "ref", value = "seen" }, } },
        { id = 68, module = "string/join", x = 1259.6875, y = 229.6875, inputs = { { name = "sep", type = "string", value = "," }, } },
        { id = 69, module = "io/print", x = 1256.89844, y = 76.8984375, inputs = { { name = "value", type = "string", value = "" }, } },
        { id = 70, module = "var/get", x = 1354.6875, y = 229.6875, inputs = { { name = "var", type = "ref", value = "names" }, } },
        { id = 71, module = "string/join", x = 1449.6875, y = 229.6875, inputs = { { name = "sep", type = "string", value = "," }, } },
        { id = 72, module = "io/print", x = 1446.89844, y = 76.8984375, inputs = { { name = "value", type = "string", value = "" }, } },
}

links = {
        { from = 50, from_pin = 0, to = 52, to_pin = 0 },
        { from = 50, from_pin = 0, to = 53, to_pin = 0 },
        { from = 52, from_pin = 0, to = 53, to_pin = 0 },
        { from = 51, from_pin = 0, to = 52, to_pin = 1 },
        { from = 51, from_pin = 0, to = 53, to_pin = 1 },
        { from = 51, from_pin = 0, to = 54, to_pin = 1 },
        { from = 51, from_pin = 0, to = 55, to_pin = 1 },
        { from = 53, from_pin = 0, to = 54, to_pin = 0 },
        { from = 54, from_pin = 0, to = 55, to_pin = 0 },
        { from = 57, from_pin = 0, to = 59, to_pin = 0 },
        { from = 58, from_pin = 0, to = 59, to_pin = 2 },
        { from = 59, from_pin = 0, to = 60, to_pin = 0 },
        { from = 60, from_pin = 0, to = 62, to_pin = 0 },
        { from = 61, from_pin = 0, to = 62, to_pin = 2 },
        { from = 62, from_pin = 0, to = 63, to_pin = 0 },
        { from = 64, from_pin = 0, to = 65, to_pin = 0 },
        { from = 65, from_pin = 0, to = 66, to_pin = 1 },
        { from = 55, from_pin = 0, to = 66, to_pin = 0 },
        { from = 63, from_pin = 0, to = 66, to_pin = 0 },
        { from = 67, from_pin = 0, to = 68, to_pin = 0 },
        { from = 68, from_pin = 0, to = 69, to_pin = 1 },
        { from = 70, from_pin = 0, to = 71, to_pin = 0 },
        { from = 71, from_pin = 0, to = 72, to_pin = 1 },
        { from = 66, from_pin = 0, to = 69, to_pin = 0 },
        { from = 69, from_pin = 0, to = 72, to_pin = 0 },
}
