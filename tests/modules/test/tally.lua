return {
        doc = "Counts its ticks, and fires `even` or `odd`",
        params = { { "step", "int", default = 1 } },
        inputs = { { "tick", "tick" } },
        outputs = { { "even", "tick" }, { "odd", "tick" }, { "count", "int" } },
        tick = function(self, input, i)
                self.n = (self.n or 0) + self.params.step
                return { count = self.n }, { self.n % 2 == 0 and "even" or "odd" }
        end,
}
