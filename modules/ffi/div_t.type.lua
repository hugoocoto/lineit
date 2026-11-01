-- ffi/div_t: C's div_t, what div() gives (see stdlib.h), a lineit type
-- with a C layout: ffi/call passes and returns it as the struct.
--   div_t div(int num, int den)
lineit_type = "0.0.1"
fields = {
        { name = "quot", type = "int32" },
        { name = "rem", type = "int32" },
}
color = { 148, 163, 184 }
