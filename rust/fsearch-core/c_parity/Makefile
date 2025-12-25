CC ?= gcc
# Ensure PCRE2 8-bit API is selected for system headers
CFLAGS ?= -O2 -DPCRE2_CODE_UNIT_WIDTH=8
LDFLAGS ?= -lpcre2-8

all: c_matcher

c_matcher: c_matcher.c
	$(CC) $(CFLAGS) -o c_matcher c_matcher.c $(LDFLAGS)

clean:
	rm -f c_matcher
