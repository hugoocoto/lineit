-- ffi/tm: C's struct tm (see time.h), as glibc lays it out, a lineit type
-- with a C layout: ffi/call passes a pointer to it, and gives it back as C
-- left it. year is since 1900, mon from 0, wday from Sunday, yday from 0.
--   time_t timegm(tm *t)        (normalizes it, and gives its time)
--   size_t strftime(char s[64], size_t n, const char *format, const tm *t)
lineit_type = "0.0.1"
fields = {
        { name = "sec", type = "int32" },
        { name = "min", type = "int32" },
        { name = "hour", type = "int32" },
        { name = "mday", type = "int32" },
        { name = "mon", type = "int32" },
        { name = "year", type = "int32" },
        { name = "wday", type = "int32" },
        { name = "yday", type = "int32" },
        { name = "isdst", type = "int32" },
        { name = "gmtoff", type = "int" },
        { name = "zone", type = "string" },
}
color = { 148, 163, 184 }
