static class LineitHost
{
    static readonly System.Text.Json.JsonSerializerOptions Opts = new() { IncludeFields = true, PropertyNameCaseInsensitive = true };

    class Sq
    {
        public System.Reflection.MethodInfo M;
        public System.Type Shared;
        public System.Type Owner;
    }

    static System.Delegate Exit(System.Type dt, System.Action<object[]> sink)
    {
        var ps = dt.GetMethod("Invoke").GetParameters()
            .Select(p => System.Linq.Expressions.Expression.Parameter(p.ParameterType, p.Name)).ToArray();
        var all = System.Linq.Expressions.Expression.NewArrayInit(typeof(object),
            ps.Select(p => System.Linq.Expressions.Expression.Convert(p, typeof(object))));
        var call = System.Linq.Expressions.Expression.Invoke(System.Linq.Expressions.Expression.Constant(sink), all);
        return System.Linq.Expressions.Expression.Lambda(dt, call, ps).Compile();
    }

    static int Main()
    {
        var utf8 = new System.Text.UTF8Encoding(false);
        var stdin = new System.IO.StreamReader(System.Console.OpenStandardInput(), utf8);
        var stdout = new System.IO.StreamWriter(System.Console.OpenStandardOutput(), utf8) { AutoFlush = true, NewLine = "\n" };
        var printed = new System.IO.StringWriter { NewLine = "\n" };
        System.Console.SetOut(printed);
        var load = System.Text.Json.JsonDocument.Parse(stdin.ReadLine()).RootElement;
        var file = load.GetProperty("file").GetString();
        var name = load.GetProperty("name").GetString();
        var mode = load.GetProperty("mode").GetString();
        var pins = load.GetProperty("returns").EnumerateArray().Select(p => p.GetString()).ToArray();
        var state = load.GetProperty("state").EnumerateArray().Select(p => p.GetString()).ToArray();
        var shared = load.GetProperty("shared").EnumerateArray().Select(p => (p[0].GetString(), p[1].GetBoolean())).ToArray();
        var then = load.GetProperty("then").GetBoolean();
        var ps = load.GetProperty("params").EnumerateArray().ToArray();
        var squares = new System.Collections.Generic.List<Sq>();

        void Send(System.Collections.Generic.Dictionary<string, object> m)
        {
            var lines = printed.ToString().Split('\n').ToList();
            if (lines.Count > 0 && lines[^1] == "") lines.RemoveAt(lines.Count - 1);
            printed.GetStringBuilder().Clear();
            if (lines.Count > 0) m["printed"] = lines;
            string json;
            try { json = System.Text.Json.JsonSerializer.Serialize(m, Opts); }
            catch (System.Exception e) when (e is System.ArgumentException || e is System.Text.Json.JsonException || e is System.NotSupportedException)
            {
                // NaN and Infinity have no JSON form (lineit has no float like them to be given)
                m.Remove("set"); m.Remove("fire");
                m["error"] = $"{file}: a value can't be given to lineit ({e.Message}): NaN and Infinity aren't supported";
                json = System.Text.Json.JsonSerializer.Serialize(m, Opts);
            }
            stdout.WriteLine(json);
        }
        string Where(System.Exception e)
        {
            var frame = new System.Diagnostics.StackTrace(e, true).GetFrames()
                .FirstOrDefault(f => System.IO.Path.GetFileName(f.GetFileName()) == file && f.GetFileLineNumber() > 0);
            return frame == null ? "" : $"{file}:{frame.GetFileLineNumber()}: ";
        }
        // a public field or property of O, by name
        object Member(object o, string n)
        {
            if (o == null) throw new System.Exception($"`{n}`: it gave null, not a value with fields");
            var t = o.GetType();
            var f = t.GetField(n);
            if (f != null) return f.GetValue(o);
            var p = t.GetProperty(n) ?? throw new System.Exception($"`{n}`: {t.Name} has no public field or property with that name");
            return p.GetValue(o);
        }
        object Field(System.Type t, string n, object set, bool write)
        {
            var f = t.GetField(n, System.Reflection.BindingFlags.Static | System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.NonPublic)
                ?? throw new System.Exception($"no field `{n}`");
            if (write) f.SetValue(null, System.Text.Json.JsonSerializer.Deserialize(((System.Text.Json.JsonElement)set).GetRawText(), f.FieldType, Opts));
            return write ? null : f.GetValue(null);
        }

        Send(new());
        for (var line = stdin.ReadLine(); line != null; line = stdin.ReadLine())
        {
            var req = System.Text.Json.JsonDocument.Parse(line).RootElement;
            try
            {
                switch (req.GetProperty("op").GetString())
                {
                    case "new":
                    {
                        var ctx = new System.Runtime.Loader.AssemblyLoadContext(null, true);
                        var asm = ctx.LoadFromAssemblyPath(typeof(LineitHost).Assembly.Location);
                        var flags = System.Reflection.BindingFlags.Static | System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.DeclaredOnly;
                        var m = asm.GetTypes().SelectMany(t => t.GetMethods(flags)).First(x => x.Name == name);
                        squares.Add(new Sq { M = m, Owner = m.DeclaringType, Shared = asm.GetType(typeof(Shared).FullName) });
                        Send(new() { ["sq"] = squares.Count - 1 });
                        break;
                    }
                    case "call":
                    {
                        var sq = squares[req.GetProperty("sq").GetInt32()];
                        var i = req.GetProperty("i");
                        foreach (var (g, _) in shared) Field(sq.Shared, g, i.GetProperty("::" + g), true);
                        var fire = new System.Collections.Generic.List<object>();
                        var mps = sq.M.GetParameters();
                        var args = new object[ps.Length];
                        for (var k = 0; k < ps.Length; k++)
                        {
                            var pname = ps[k].GetProperty("name").GetString();
                            var exit = ps[k].GetProperty("exit");
                            if (exit.ValueKind == System.Text.Json.JsonValueKind.Null)
                                args[k] = System.Text.Json.JsonSerializer.Deserialize(i.GetProperty(pname).GetRawText(), mps[k].ParameterType, Opts);
                            else
                            {
                                var outs = exit.EnumerateArray().Select(p => p.GetString()).ToArray();
                                var shape = ps[k].GetProperty("shape").GetString();
                                args[k] = Exit(mps[k].ParameterType, v =>
                                {
                                    var d = new System.Collections.Generic.Dictionary<string, object>();
                                    // the values: the arguments, a tuple's elements or the fields of one object
                                    if (shape == "args") for (var n = 0; n < outs.Length; n++) d[outs[n]] = v[n];
                                    else if (shape == "tuple") { var t = (System.Runtime.CompilerServices.ITuple)v[0]; for (var n = 0; n < outs.Length; n++) d[outs[n]] = t[n]; }
                                    else foreach (var pin in outs) d[pin] = Member(v[0], pin);
                                    fire.Add(new object[] { pname, d });
                                });
                            }
                        }
                        object r;
                        try { r = sq.M.Invoke(null, args); }
                        catch (System.Reflection.TargetInvocationException e) { System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(e.InnerException).Throw(); throw; }
                        var set = new System.Collections.Generic.Dictionary<string, object>();
                        if (mode == "one") set[pins[0]] = r;
                        else if (mode == "tuple") { if (r == null) throw new System.Exception("it gave null, not a tuple"); var t = (System.Runtime.CompilerServices.ITuple)r; for (var n = 0; n < pins.Length; n++) set[pins[n]] = t[n]; }
                        else if (mode == "fields")
                            foreach (var pin in pins) set[pin] = Member(r, pin);
                        foreach (var (g, sets) in shared) if (sets) set["::" + g] = Field(sq.Shared, g, null, false);
                        if (then) fire.Add(new object[] { "then", new System.Collections.Generic.Dictionary<string, object>() });
                        Send(new() { ["set"] = set, ["fire"] = fire });
                        break;
                    }
                    case "state":
                    {
                        var sq = squares[req.GetProperty("sq").GetInt32()];
                        Send(new() { ["state"] = state.ToDictionary(s => s, s => Field(sq.Owner, s, null, false)) });
                        break;
                    }
                    case "set_state":
                    {
                        var sq = squares[req.GetProperty("sq").GetInt32()];
                        foreach (var p in req.GetProperty("state").EnumerateObject()) Field(sq.Owner, p.Name, p.Value, true);
                        Send(new());
                        break;
                    }
                }
            }
            catch (System.Exception e)
            {
                Send(new() { ["error"] = $"{Where(e)}{e.GetType().Name}: {e.Message}" });
            }
        }
        return 0;
    }
}
