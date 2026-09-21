SRC = $(wildcard src/*.c)
OUT = lineit

# conf.h needs Lua 5.1 or LuaJIT (both implement the Lua 5.1 API).
# Override with: make LUA=luajit
LUA ?= lua5.1
LUA_CFLAGS = $(shell pkg-config --cflags $(LUA) 2>/dev/null)
LUA_LIBS = $(shell pkg-config --libs $(LUA) 2>/dev/null || echo '-l$(LUA) -lm')

CFLAGS += -I./raylib-6.0_linux_amd64/include/
CFLAGS += -I./thirdparty/flag.h -I./thirdparty/conf.h $(LUA_CFLAGS)

DEPS += ./raylib-6.0_linux_amd64/lib/libraylib.a -lX11 -lm
DEPS += $(LUA_LIBS)

all: $(OUT)

$(OUT): $(SRC)
	$(CC) $(CFLAGS) $^ $(DEPS) -o $@

clean:
	rm -f $(OUT)

.PHONY: all clean
