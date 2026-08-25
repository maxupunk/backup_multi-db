#!/usr/bin/env python3
"""Remove a chave `edition` dos alvos da receita gerada pelo cargo-chef.

Ao rodar `cargo chef prepare`, o cargo-chef nao copia os `Cargo.toml` do
repositorio: ele os desserializa e serializa de volta para dentro do
`recipe.json`. Nesse round-trip a crate `cargo_toml` preenche os defaults de
cada alvo, e um deles e' o `edition` — que passa a aparecer em `[lib]`,
`[[bin]]`, `[[test]]` e `[[example]]`.

O Cargo 1.9x deprecou `edition` nesse nivel (so' `[package]` deve declara-lo),
entao o `cargo chef cook` do stage seguinte despeja um bloco de

    warning: /app/Cargo.toml: `edition` is set on library `backend` which is
    deprecated

a cada build. O ruido nao vem de nenhum manifesto deste repositorio: os
`Cargo.toml` versionados aqui so' declaram `edition` em `[package]`.

Upstream: https://github.com/LukeMathWalker/cargo-chef/issues/350 (aberto).
Enquanto nao sai a correcao, a receita e' higienizada aqui. O `edition` de
`[package]` e' preservado — e' ele que de fato seleciona a edicao.
"""

import json
import re
import sys

# Tabelas de alvo. `[package]` fica de fora de proposito.
TARGET_TABLES = ("[lib]", "[[bin]]", "[[test]]", "[[example]]", "[[bench]]")

EDITION_KEY = re.compile(r"edition\s*=")


def strip(contents: str) -> tuple[str, int]:
    table, kept, removed = "", [], 0
    for line in contents.splitlines():
        stripped = line.strip()
        if stripped.startswith("["):
            table = stripped
        if table in TARGET_TABLES and EDITION_KEY.match(stripped):
            removed += 1
            continue
        kept.append(line)
    return "\n".join(kept) + "\n", removed


def main(path: str) -> int:
    with open(path, encoding="utf-8") as handle:
        recipe = json.load(handle)

    total = 0
    for manifest in recipe["skeleton"]["manifests"]:
        manifest["contents"], removed = strip(manifest["contents"])
        total += removed

    with open(path, "w", encoding="utf-8") as handle:
        json.dump(recipe, handle, indent=2)

    print(f"strip-target-edition: {total} chave(s) `edition` removida(s) de {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "recipe.json"))
