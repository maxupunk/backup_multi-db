//! Paginacao no formato do `SimplePaginator` do Lucid.
//!
//! Duas rotas da Fase 5 devolvem uma pagina — `GET /api/users` e
//! `GET /api/audit-logs` — e as duas expoem o **mesmo** bloco `meta`, com nove
//! chaves em camelCase. O front-end consome `lastPage` e `nextPageUrl` para
//! decidir se ainda ha' pagina, entao inventar um shape mais enxuto aqui
//! quebraria a paginacao da interface inteira.
//!
//! As URLs sao relativas e ignoram os filtros da requisicao (`/?page=2`), o que
//! parece um bug do Lucid mas e' o comportamento gravado nos golden files da
//! Fase 2. Reproduzir e' obrigacao; corrigir seria mudanca de contrato.

use serde::{Deserialize, Serialize};

/// Bloco `meta` de uma pagina.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMeta {
    pub total: u64,
    pub per_page: u64,
    pub current_page: u64,
    pub last_page: u64,
    pub first_page: u64,
    pub first_page_url: String,
    pub last_page_url: String,
    pub next_page_url: Option<String>,
    pub previous_page_url: Option<String>,
}

impl PageMeta {
    /// Monta o `meta` a partir do total e da janela pedida.
    ///
    /// `per_page` zero viraria divisao por zero no calculo de `last_page`; a
    /// normalizacao para 1 acontece antes, em [`PageRequest`], para que o valor
    /// que chega aqui ja' seja o mesmo que a consulta usou.
    pub fn new(total: u64, page: PageRequest) -> Self {
        let per_page = page.per_page.max(1);
        // `max(1)`: uma lista vazia ainda tem uma pagina — a primeira. Zero
        // faria o front-end concluir que nem a pagina atual existe.
        let last_page = total.div_ceil(per_page).max(1);
        let current_page = page.page.max(1);

        Self {
            total,
            per_page,
            current_page,
            last_page,
            first_page: 1,
            first_page_url: page_url(1),
            last_page_url: page_url(last_page),
            next_page_url: (current_page < last_page).then(|| page_url(current_page + 1)),
            previous_page_url: (current_page > 1).then(|| page_url(current_page - 1)),
        }
    }
}

fn page_url(page: u64) -> String {
    format!("/?page={page}")
}

/// Uma pagina completa: os itens e o `meta`.
///
/// `GET /api/users` devolve exatamente isto no topo da resposta;
/// `GET /api/audit-logs` embrulha os dois campos junto de `success`. Por isso
/// os campos sao publicos em vez de a struct so' saber se serializar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page<T> {
    pub meta: PageMeta,
    pub data: Vec<T>,
}

impl<T> Page<T> {
    pub fn new(data: Vec<T>, total: u64, page: PageRequest) -> Self {
        Self {
            meta: PageMeta::new(total, page),
            data,
        }
    }
}

/// Janela pedida pelo cliente, ja' saneada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    pub page: u64,
    pub per_page: u64,
}

impl PageRequest {
    /// Sanea `?page=` e `?limit=` vindos da query string.
    ///
    /// Valor ausente ou impossivel de interpretar cai no default em vez de virar
    /// 422: e' o que o Lucid faz, e um cliente que manda `?page=abc` recebe a
    /// primeira pagina, nao um erro.
    ///
    /// `max_per_page` existe porque `GET /api/audit-logs` limita a 100 por
    /// pagina. Sem o teto, `?limit=1000000` seria um jeito barato de derrubar o
    /// processo pela memoria.
    pub fn from_query(
        page: Option<&str>,
        limit: Option<&str>,
        default_per_page: u64,
        max_per_page: Option<u64>,
    ) -> Self {
        let page = page.and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(1);
        let per_page = limit
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(default_per_page);

        Self {
            page: page.max(1),
            per_page: max_per_page
                .map_or(per_page, |max| per_page.min(max))
                .max(1),
        }
    }

    /// Deslocamento correspondente, para o `OFFSET` da consulta.
    pub const fn offset(self) -> u64 {
        (self.page - 1) * self.per_page
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(page: u64, per_page: u64) -> PageRequest {
        PageRequest { page, per_page }
    }

    #[test]
    fn a_single_page_has_no_neighbours() {
        let meta = PageMeta::new(9, request(1, 10));

        assert_eq!(meta.last_page, 1);
        assert_eq!(meta.next_page_url, None);
        assert_eq!(meta.previous_page_url, None);
        assert_eq!(meta.first_page_url, "/?page=1");
        assert_eq!(meta.last_page_url, "/?page=1");
    }

    #[test]
    fn links_the_neighbouring_pages() {
        let meta = PageMeta::new(9, request(2, 2));

        assert_eq!(meta.last_page, 5);
        assert_eq!(meta.next_page_url.as_deref(), Some("/?page=3"));
        assert_eq!(meta.previous_page_url.as_deref(), Some("/?page=1"));
        assert_eq!(meta.last_page_url, "/?page=5");
    }

    #[test]
    fn an_empty_list_still_reports_one_page() {
        // `lastPage: 0` faria o front-end concluir que nem a pagina atual
        // existe e esconder a tabela inteira.
        let meta = PageMeta::new(0, request(1, 10));
        assert_eq!(meta.last_page, 1);
    }

    #[test]
    fn rounds_the_last_page_up() {
        assert_eq!(PageMeta::new(11, request(1, 10)).last_page, 2);
        assert_eq!(PageMeta::new(20, request(1, 10)).last_page, 2);
        assert_eq!(PageMeta::new(21, request(1, 10)).last_page, 3);
    }

    #[test]
    fn serializes_in_camel_case() {
        let json = serde_json::to_value(PageMeta::new(0, request(1, 10))).unwrap();

        for key in [
            "total",
            "perPage",
            "currentPage",
            "lastPage",
            "firstPage",
            "firstPageUrl",
            "lastPageUrl",
            "nextPageUrl",
            "previousPageUrl",
        ] {
            assert!(json.get(key).is_some(), "faltou `{key}` no meta");
        }
        assert_eq!(
            json.as_object().map(serde_json::Map::len),
            Some(9),
            "chave a mais ou a menos no meta"
        );
    }

    #[test]
    fn null_neighbours_are_emitted_not_omitted() {
        // O golden traz `"nextPageUrl": null`. Omitir a chave seria diferenca.
        let json = serde_json::to_value(PageMeta::new(0, request(1, 10))).unwrap();
        assert!(json["nextPageUrl"].is_null());
        assert!(json["previousPageUrl"].is_null());
    }

    #[test]
    fn falls_back_instead_of_rejecting_a_bad_query() {
        let parsed = PageRequest::from_query(Some("abc"), Some(""), 10, None);
        assert_eq!(parsed, request(1, 10));
    }

    #[test]
    fn clamps_the_page_size_to_the_ceiling() {
        // `GET /api/audit-logs` limita a 100. Sem o teto, `?limit=1000000`
        // seria um jeito barato de derrubar o processo pela memoria.
        let parsed = PageRequest::from_query(Some("1"), Some("1000000"), 50, Some(100));
        assert_eq!(parsed.per_page, 100);
    }

    #[test]
    fn refuses_page_zero() {
        // `?page=0` daria offset negativo.
        assert_eq!(PageRequest::from_query(Some("0"), None, 10, None).page, 1);
        assert_eq!(
            PageRequest::from_query(Some("0"), None, 10, None).offset(),
            0
        );
    }

    #[test]
    fn computes_the_offset() {
        assert_eq!(request(1, 10).offset(), 0);
        assert_eq!(request(3, 10).offset(), 20);
    }
}
