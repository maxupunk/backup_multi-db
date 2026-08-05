#!/bin/bash
# Testes de configure_max_old_space (backend/docker-memory.sh).
#
# Rodam sem container: os caminhos de cgroup são injetáveis, então cada caso
# escreve um arquivo temporário com o conteúdo que o kernel escreveria.
#
#   bash backend/tests/shell/docker_memory.test.sh

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB_PATH="$SCRIPT_DIR/../../docker-memory.sh"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

failures=0
total=0

# Executa configure_max_old_space num subshell limpo e ecoa "<mb>|<origem>".
#
# O subshell importa: a lib guarda estado em variáveis globais e o `.` só lê os
# defaults com `:-` uma vez. Sem isolamento, um caso vazaria configuração no
# seguinte e os testes passariam por acidente.
run_case() {
    local v2_content="$1"
    local v1_content="$2"
    local env_override="$3"
    local ceiling="${4:-}"

    local v2_path="$WORK_DIR/memory.max"
    local v1_path="$WORK_DIR/memory.limit_in_bytes"

    rm -f "$v2_path" "$v1_path"
    [ "$v2_content" != "ABSENT" ] && printf '%s\n' "$v2_content" > "$v2_path"
    [ "$v1_content" != "ABSENT" ] && printf '%s\n' "$v1_content" > "$v1_path"

    (
        export CGROUP_V2_LIMIT_PATH="$v2_path"
        export CGROUP_V1_LIMIT_PATH="$v1_path"
        export NODE_MAX_OLD_SPACE_MB="$env_override"
        [ -n "$ceiling" ] && export HEAP_CEILING_MB="$ceiling"

        # shellcheck disable=SC1090
        . "$LIB_PATH"
        configure_max_old_space
        printf '%s|%s' "$MAX_OLD_SPACE_MB" "$MAX_OLD_SPACE_ORIGIN"
    )
}

expect_mb() {
    local description="$1"
    local expected_mb="$2"
    local actual="$3"
    local actual_mb="${actual%%|*}"

    total=$(( total + 1 ))

    if [ "$actual_mb" = "$expected_mb" ]; then
        printf '  ok   %s\n' "$description"
    else
        printf '  FAIL %s\n       esperado %sMB, obtido %sMB (origem: %s)\n' \
            "$description" "$expected_mb" "$actual_mb" "${actual#*|}"
        failures=$(( failures + 1 ))
    fi
}

echo "configure_max_old_space"

# 512 MiB * 65% = 332 (o valor fixo anterior era 320).
expect_mb "cgroup v2 com 512M -> 65%" 332 \
    "$(run_case 536870912 ABSENT '')"

expect_mb "cgroup v2 com 1G -> acompanha o limite" 665 \
    "$(run_case 1073741824 ABSENT '')"

expect_mb "cgroup v2 sem limite (max) -> fallback" 320 \
    "$(run_case max ABSENT '')"

expect_mb "cgroup v1 quando o v2 nao existe" 332 \
    "$(run_case ABSENT 536870912 '')"

# 0x7FFFFFFFFFFFF000: sentinela de "sem limite" do cgroup v1.
expect_mb "sentinela do cgroup v1 nao vira limite" 320 \
    "$(run_case ABSENT 9223372036854771712 '')"

expect_mb "nenhum arquivo de cgroup -> fallback" 320 \
    "$(run_case ABSENT ABSENT '')"

expect_mb "arquivo vazio -> fallback" 320 \
    "$(run_case '' ABSENT '')"

expect_mb "conteudo inesperado -> fallback" 320 \
    "$(run_case 'nao-e-numero' ABSENT '')"

expect_mb "limite zero -> fallback" 320 \
    "$(run_case 0 ABSENT '')"

expect_mb "override explicito vence a deteccao" 999 \
    "$(run_case 536870912 ABSENT 999)"

expect_mb "override vence tambem sem cgroup" 999 \
    "$(run_case ABSENT ABSENT 999)"

# "320MB" mataria o node no boot com uma mensagem que nao cita a variavel.
expect_mb "override com unidade e ignorado, cai na deteccao" 332 \
    "$(run_case 536870912 ABSENT '320MB')"

expect_mb "override nao numerico sem cgroup cai no fallback" 320 \
    "$(run_case ABSENT ABSENT 'auto')"

# 8 GiB * 65% = 5324, acima do teto.
expect_mb "teto limita heap gigante" 2048 \
    "$(run_case 8589934592 ABSENT '')"

expect_mb "teto e configuravel" 512 \
    "$(run_case 8589934592 ABSENT '' 512)"

# O v2 tem precedência: com os dois presentes, o v1 é ignorado.
expect_mb "v2 tem precedencia sobre v1" 332 \
    "$(run_case 536870912 1073741824 '')"

echo
if [ "$failures" -eq 0 ]; then
    printf 'PASSED  %s testes\n' "$total"
    exit 0
fi

printf 'FAILED  %s de %s testes\n' "$failures" "$total"
exit 1
