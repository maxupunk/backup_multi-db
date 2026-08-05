#!/bin/bash
# Resolve o --max-old-space-size do V8 a partir do limite de memória do container.
#
# Sourceado pelo docker-entrypoint.sh. Fica em arquivo separado para poder ser
# exercitado por teste (tests/shell/docker_memory.test.sh) sem disparar as
# migrations do entrypoint.
#
# Contrato: define MAX_OLD_SPACE_MB e MAX_OLD_SPACE_ORIGIN.

# Fração do limite do container destinada ao old space do V8.
#
# O restante NÃO é folga desperdiçada: --max-old-space-size limita o old space,
# não o RSS. Fora dele ainda contam young generation, code space, buffers
# nativos (gzip, pipes dos processos de dump, better-sqlite3), stacks de thread
# e o overhead do próprio processo. É essa parte que o OOM killer soma junto.
HEAP_FRACTION_PERCENT="${HEAP_FRACTION_PERCENT:-65}"

# Sem limite de cgroup não há de que tirar uma fração — 65% da RAM de um host
# de 64 GB seria um número sem qualquer relação com a carga real.
HEAP_FALLBACK_MB="${HEAP_FALLBACK_MB:-320}"

# Teto. A carga deste sistema é toda streaming com picos limitados (F1–F3 do
# MEMORY_ROADMAP.md): um old space gigante não acelera nada, alonga as pausas de
# GC e faria o heap snapshot automático ocupar vários GB. Quem quiser mais passa
# NODE_MAX_OLD_SPACE_MB explicitamente.
HEAP_CEILING_MB="${HEAP_CEILING_MB:-2048}"

CGROUP_V2_LIMIT_PATH="${CGROUP_V2_LIMIT_PATH:-/sys/fs/cgroup/memory.max}"
CGROUP_V1_LIMIT_PATH="${CGROUP_V1_LIMIT_PATH:-/sys/fs/cgroup/memory/memory.limit_in_bytes}"

# O cgroup v1 grava 0x7FFFFFFFFFFFF000 quando não há limite configurado.
# Qualquer valor acima de 2^53 é "sem limite" na prática — mesmo critério do
# ContainerMemoryProbe (backend/app/services/container_memory_probe.ts), para
# que o entrypoint e o painel não discordem sobre o que é um limite real.
UNLIMITED_THRESHOLD_BYTES=9007199254740992

# Ecoa o limite de memória do container em bytes, ou nada quando não há limite.
detect_memory_limit_bytes() {
    local raw=""

    if [ -r "$CGROUP_V2_LIMIT_PATH" ]; then
        raw="$(tr -d '[:space:]' < "$CGROUP_V2_LIMIT_PATH")"
    elif [ -r "$CGROUP_V1_LIMIT_PATH" ]; then
        raw="$(tr -d '[:space:]' < "$CGROUP_V1_LIMIT_PATH")"
    fi

    # Cobre "max" (v2 sem limite), arquivo vazio e qualquer conteúdo inesperado.
    case "$raw" in
        '' | *[!0-9]*) return 0 ;;
    esac

    if [ "$raw" -le 0 ] || [ "$raw" -ge "$UNLIMITED_THRESHOLD_BYTES" ]; then
        return 0
    fi

    printf '%s' "$raw"
}

configure_max_old_space() {
    # Override explícito vence — desde que seja um número de MB. Um "320MB" ou
    # "0.5G" faria o node morrer no boot com uma mensagem que não aponta para a
    # variável de ambiente; melhor ignorar e dizer por quê.
    if [ -n "${NODE_MAX_OLD_SPACE_MB:-}" ]; then
        case "$NODE_MAX_OLD_SPACE_MB" in
            '' | *[!0-9]*)
                MAX_OLD_SPACE_INVALID_OVERRIDE="$NODE_MAX_OLD_SPACE_MB"
                ;;
            *)
                MAX_OLD_SPACE_MB="$NODE_MAX_OLD_SPACE_MB"
                MAX_OLD_SPACE_ORIGIN="NODE_MAX_OLD_SPACE_MB"
                return 0
                ;;
        esac
    fi

    local limit_bytes
    limit_bytes="$(detect_memory_limit_bytes)"

    if [ -z "$limit_bytes" ]; then
        MAX_OLD_SPACE_MB="$HEAP_FALLBACK_MB"
        MAX_OLD_SPACE_ORIGIN="padrão (nenhum limite de memória no cgroup)"
        return 0
    fi

    local limit_mb=$(( limit_bytes / 1048576 ))

    MAX_OLD_SPACE_MB=$(( limit_mb * HEAP_FRACTION_PERCENT / 100 ))
    MAX_OLD_SPACE_ORIGIN="${HEAP_FRACTION_PERCENT}% de ${limit_mb}MB do container"

    if [ "$MAX_OLD_SPACE_MB" -gt "$HEAP_CEILING_MB" ]; then
        MAX_OLD_SPACE_ORIGIN="teto de ${HEAP_CEILING_MB}MB (${HEAP_FRACTION_PERCENT}% de ${limit_mb}MB daria ${MAX_OLD_SPACE_MB}MB)"
        MAX_OLD_SPACE_MB="$HEAP_CEILING_MB"
    fi
}
