#include "conf.h"
#include "cum.h"
#include "flag.h"
#include "raylib.h"

#include <libgen.h>
#include <stdint.h>
#include <unistd.h>

#ifndef VERSION
#define VERSION "unknown"
#endif

int
main(int argc, char **argv)
{
        const char *f_version;

        flag_program();
        flag_add(&f_version, "--version");
        if (flag_parse(&argc, &argv)) {
                flag_show_help(STDERR_FILENO);
                exit(1);
        }

        if (f_version) {
                fprintf(stdout, "%s, version %s\n", basename(argv[0]), VERSION);
                fprintf(stdout, "Copyright (C) 2026 Hugo Coto Flórez.\n");
                fprintf(stdout, "License GPLv3+: GNU GPL version 3 or later <http://gnu.org/licenses/gpl.html>\n");
                fprintf(stdout, "\n");
                fprintf(stdout, "This is free software; you are free to change and redistribute it.\n");
                fprintf(stdout, "There is NO WARRANTY, to the extent permitted by law.\n");
                exit(0);
        }

        flag_free();
        return 0;
}
