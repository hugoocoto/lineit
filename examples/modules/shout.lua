-- A Lua module: it says its pins, and `eval` computes its outputs from its
-- inputs. Change it while the editor is open and the canvas picks it up.
return {
        doc = "The text in capitals, with as many ! as you say",
        inputs = { { "text", "str", default = "hello" }, { "times", "int", default = 3 } },
        outputs = { { "result", "str" } },
        eval = function(i)
                return { result = i.text:upper() .. string.rep("!", i.times) }
        end,
}
