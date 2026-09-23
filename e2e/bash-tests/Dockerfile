# Environment for bash's own regression suite (tests/run-*). Bash is built from source, pinned
# to the bash-5.3 release: the build provides the helper binaries the suite needs (recho, zecho,
# printenv, xcase) and a reference bash whose output matches the `.right` files, which is what
# `--baseline` runs.
FROM ubuntu:25.10
# When bumping this (and the tag below), run `cargo xtask test e2e --baseline bash-tests` first: it
# catches what brush runs cannot, such as a new test needing a locale or tool this image lacks.
# Then run against brush and refresh xfail-list.txt; the runner flags new failures and stale entries.
ARG BASH_REF=b8c60bc9ca365f8261fa97900b6fa939f6ebc303
RUN apt-get update && apt-get install -y --no-install-recommends \
        bsdextrautils build-essential ca-certificates git locales python3-pytest \
        python3-pytest-xdist \
    && rm -rf /var/lib/apt/lists/*
# Every locale the tests switch to, legacy charsets included: they check `locale -a` for these
# names. SHIFT_JIS is not ASCII-compatible, which localedef warns about unless told not to.
RUN for spec in en_US:UTF-8 de_DE:UTF-8 fr_FR:ISO-8859-1 ru_RU:CP1251 zh_TW:BIG5 \
            zh_HK:BIG5-HKSCS ja_JP:SHIFT_JIS; do \
        localedef --no-warnings=ascii -i "${spec%:*}" -f "${spec#*:}" \
            "${spec%:*}.$(echo "${spec#*:}" | sed 's/SHIFT_JIS/SJIS/')" || exit 1; \
    done
# Savannah serves no uploadpack for arbitrary SHAs, so clone the tag and check it is the commit.
RUN git clone -q --depth 1 --branch bash-5.3 https://git.savannah.gnu.org/git/bash.git /bash \
    && test "$(git -C /bash rev-parse HEAD)" = "$BASH_REF" \
    && cd /bash && ./configure -q && make -s -j"$(nproc)" \
    && make -s recho zecho printenv xcase && cp recho zecho printenv xcase tests/ \
    && chmod -R a+rX /bash
COPY bash-tests/entrypoint.sh bash-tests/skip-list.txt /e2e/
COPY bash-tests/tests /e2e/tests
COPY lib /e2e/lib
ENV LANG=C.UTF-8 PYTHONPATH=/e2e/lib
ENTRYPOINT ["/e2e/entrypoint.sh"]
