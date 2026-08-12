//! Normalizacao de e-mail equivalente ao `normalizeEmail()` do VineJS.
//!
//! Os validators de `register` e `login` da implementacao anterior aplicam `normalizeEmail()`
//! antes de tocar no banco. Por baixo e' o `normalizeEmail` do `validator.js`
//! com as opcoes default, e ele faz **mais** que baixar a caixa: para os
//! provedores conhecidos ele remove o subendereco (`+tag`), e para o Gmail
//! remove tambem os pontos do local-part e converte `googlemail.com` em
//! `gmail.com`.
//!
//! Reproduzir isso nao e' capricho. O banco migrado guarda o endereco **ja'
//! normalizado**; quem se cadastrou como `j.o.a.o+erp@gmail.com` esta' gravado
//! como `joao@gmail.com`. Se o backend so' baixasse a caixa, essa pessoa
//! digitaria o mesmo e-mail de sempre e receberia "Invalid user credentials",
//! sem nenhuma pista do motivo — e o suporte procuraria o problema na senha.
//!
//! O algoritmo e as listas de dominio vem do `validator.js`. Divergir dele em
//! um dominio significa divergir da implementacao anterior naquele dominio.

/// Dominios tratados como Gmail. `googlemail.com` e' um alias historico.
const GMAIL_DOMAINS: [&str; 2] = ["gmail.com", "googlemail.com"];

/// Dominios da Apple.
const ICLOUD_DOMAINS: [&str; 2] = ["icloud.com", "me.com"];

/// Dominios da Microsoft. Lista ordenada — a busca e' binaria.
const OUTLOOK_DOMAINS: [&str; 75] = [
    "hotmail.at",
    "hotmail.be",
    "hotmail.ca",
    "hotmail.cl",
    "hotmail.co.il",
    "hotmail.co.nz",
    "hotmail.co.th",
    "hotmail.co.uk",
    "hotmail.com",
    "hotmail.com.ar",
    "hotmail.com.au",
    "hotmail.com.br",
    "hotmail.com.gr",
    "hotmail.com.mx",
    "hotmail.com.pe",
    "hotmail.com.tr",
    "hotmail.com.vn",
    "hotmail.cz",
    "hotmail.de",
    "hotmail.dk",
    "hotmail.es",
    "hotmail.fr",
    "hotmail.hu",
    "hotmail.id",
    "hotmail.ie",
    "hotmail.in",
    "hotmail.it",
    "hotmail.jp",
    "hotmail.kr",
    "hotmail.lv",
    "hotmail.my",
    "hotmail.ph",
    "hotmail.pt",
    "hotmail.sa",
    "hotmail.sg",
    "hotmail.sk",
    "live.at",
    "live.be",
    "live.cl",
    "live.co.uk",
    "live.com",
    "live.com.ar",
    "live.com.mx",
    "live.de",
    "live.dk",
    "live.fr",
    "live.hk",
    "live.ie",
    "live.in",
    "live.it",
    "live.jp",
    "live.nl",
    "live.no",
    "live.ph",
    "live.ru",
    "live.se",
    "live.sg",
    "outlook.at",
    "outlook.be",
    "outlook.cl",
    "outlook.co.id",
    "outlook.co.il",
    "outlook.co.nz",
    "outlook.co.th",
    "outlook.com",
    "outlook.com.au",
    "outlook.com.br",
    "outlook.com.gr",
    "outlook.com.pe",
    "outlook.com.tr",
    "outlook.com.vn",
    "outlook.cz",
    "outlook.de",
    "outlook.dk",
    "passport.com",
];

/// Dominios do Yahoo. Lista ordenada — a busca e' binaria.
const YAHOO_DOMAINS: [&str; 44] = [
    "rocketmail.com",
    "yahoo.ca",
    "yahoo.co.id",
    "yahoo.co.in",
    "yahoo.co.jp",
    "yahoo.co.kr",
    "yahoo.co.nz",
    "yahoo.co.th",
    "yahoo.co.uk",
    "yahoo.co.za",
    "yahoo.com",
    "yahoo.com.ar",
    "yahoo.com.au",
    "yahoo.com.br",
    "yahoo.com.co",
    "yahoo.com.hk",
    "yahoo.com.hr",
    "yahoo.com.mx",
    "yahoo.com.my",
    "yahoo.com.pe",
    "yahoo.com.ph",
    "yahoo.com.sg",
    "yahoo.com.tr",
    "yahoo.com.tw",
    "yahoo.com.ua",
    "yahoo.com.ve",
    "yahoo.com.vn",
    "yahoo.cz",
    "yahoo.de",
    "yahoo.dk",
    "yahoo.es",
    "yahoo.fi",
    "yahoo.fr",
    "yahoo.gr",
    "yahoo.hu",
    "yahoo.ie",
    "yahoo.in",
    "yahoo.it",
    "yahoo.nl",
    "yahoo.no",
    "yahoo.pl",
    "yahoo.pt",
    "yahoo.ro",
    "yahoo.se",
];

/// Normaliza um e-mail como o VineJS faz antes de consultar o banco.
///
/// Uma entrada sem `@`, ou cujo local-part desapareceria na normalizacao,
/// volta apenas com os espacos aparados e em minusculas: o formato ja' foi (ou
/// sera') recusado pela validacao, e devolver `None` daria ao chamador um caso
/// a mais para tratar sem nenhum ganho.
pub fn normalize(email: &str) -> String {
    let trimmed = email.trim();

    // O local-part pode conter `@` entre aspas; o dominio e' o que vem depois
    // do **ultimo**, e e' assim que o `validator.js` separa.
    let Some((local, domain)) = trimmed.rsplit_once('@') else {
        return trimmed.to_lowercase();
    };

    let domain = domain.to_lowercase();

    let (local, domain) = if GMAIL_DOMAINS.contains(&domain.as_str()) {
        let local = strip_plus_subaddress(local).replace('.', "");
        (local, "gmail.com".to_string())
    } else if ICLOUD_DOMAINS.contains(&domain.as_str())
        || OUTLOOK_DOMAINS.binary_search(&domain.as_str()).is_ok()
    {
        (strip_plus_subaddress(local).to_string(), domain)
    } else if YAHOO_DOMAINS.binary_search(&domain.as_str()).is_ok() {
        (strip_dash_subaddress(local).to_string(), domain)
    } else {
        (local.to_string(), domain)
    };

    if local.is_empty() {
        // `+tag@gmail.com` normalizaria para `@gmail.com`. O `validator.js`
        // devolve `false` aqui; devolver a entrada crua mantem a mensagem de
        // erro no lugar certo — a validacao de formato.
        return trimmed.to_lowercase();
    }

    format!("{}@{}", local.to_lowercase(), domain)
}

/// Corta o subendereco `+tag`, usado por Gmail, Outlook e iCloud.
fn strip_plus_subaddress(local: &str) -> &str {
    local.split('+').next().unwrap_or(local)
}

/// Corta o subendereco do Yahoo, que separa com `-` em vez de `+`.
///
/// Só o **ultimo** trecho e' o rotulo: `maria-santos-erp` vira `maria-santos`,
/// e nao `maria`.
fn strip_dash_subaddress(local: &str) -> &str {
    match local.rfind('-') {
        Some(index) if index > 0 => &local[..index],
        _ => local,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_an_ordinary_address() {
        assert_eq!(normalize("  Admin@Contract.TEST "), "admin@contract.test");
    }

    #[test]
    fn keeps_dots_outside_gmail() {
        // Fora do Gmail o ponto e' significativo: apagar mudaria a caixa postal.
        assert_eq!(
            normalize("j.o.a.o@empresa.com.br"),
            "j.o.a.o@empresa.com.br"
        );
    }

    #[test]
    fn removes_gmail_dots_and_subaddress() {
        // E' o caso que motiva o modulo: o banco migrado guarda `joao@gmail.com`,
        // e a pessoa digita o endereco pontuado que sempre usou.
        assert_eq!(normalize("j.o.a.o+erp@gmail.com"), "joao@gmail.com");
    }

    #[test]
    fn converts_googlemail_to_gmail() {
        assert_eq!(
            normalize("Joao.Silva@googlemail.com"),
            "joaosilva@gmail.com"
        );
    }

    #[test]
    fn removes_the_subaddress_of_outlook_and_icloud() {
        assert_eq!(normalize("maria+erp@outlook.com"), "maria@outlook.com");
        assert_eq!(
            normalize("maria+erp@hotmail.com.br"),
            "maria@hotmail.com.br"
        );
        assert_eq!(normalize("maria+erp@icloud.com"), "maria@icloud.com");
        // O ponto continua valendo fora do Gmail, inclusive nesses dominios.
        assert_eq!(normalize("ma.ria@outlook.com"), "ma.ria@outlook.com");
    }

    #[test]
    fn removes_only_the_last_dash_segment_on_yahoo() {
        assert_eq!(
            normalize("maria-santos-erp@yahoo.com"),
            "maria-santos@yahoo.com"
        );
        assert_eq!(normalize("maria@yahoo.com.br"), "maria@yahoo.com.br");
    }

    #[test]
    fn keeps_a_leading_dash_on_yahoo() {
        // `-erp@yahoo.com` nao pode virar `@yahoo.com`.
        assert_eq!(normalize("-erp@yahoo.com"), "-erp@yahoo.com");
    }

    #[test]
    fn splits_on_the_last_at_sign() {
        assert_eq!(normalize("\"a@b\"@Empresa.com"), "\"a@b\"@empresa.com");
    }

    #[test]
    fn survives_input_that_is_not_an_address() {
        // Quem recusa o formato e' a validacao; aqui so' nao pode entrar em panico.
        assert_eq!(normalize("sem-arroba"), "sem-arroba");
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("+erp@gmail.com"), "+erp@gmail.com");
    }

    #[test]
    fn is_idempotent_wherever_the_separator_is_plus() {
        for input in [
            "j.o.a.o+erp@gmail.com",
            "Admin@Contract.TEST",
            "maria+erp@outlook.com",
            "maria+erp@icloud.com",
        ] {
            let once = normalize(input);
            assert_eq!(normalize(&once), once, "nao e' idempotente para {input}");
        }
    }

    #[test]
    fn the_yahoo_rule_is_not_idempotent_and_that_is_correct() {
        // `maria-santos-erp` -> `maria-santos` -> `maria`. O `validator.js` se
        // comporta igual, porque o `-` tambem e' um caractere legitimo de
        // nome, e a regra nao tem como distinguir os dois casos.
        //
        // Nao e' um problema na pratica: o que se normaliza e' sempre o que a
        // pessoa **digitou**, nunca o que ja' esta' gravado. Cadastro e login
        // aplicam a mesma transformacao ao mesmo texto e chegam ao mesmo lugar.
        //
        // O teste existe para que a nao-idempotencia seja uma decisao
        // registrada, e nao uma descoberta durante um incidente.
        assert_eq!(
            normalize("maria-santos-erp@yahoo.com"),
            "maria-santos@yahoo.com"
        );
        assert_eq!(normalize("maria-santos@yahoo.com"), "maria@yahoo.com");
    }

    #[test]
    fn the_domain_lists_are_sorted() {
        // A busca e' binaria: uma lista fora de ordem faria dominios reais
        // deixarem de ser reconhecidos, em silencio.
        assert!(OUTLOOK_DOMAINS.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(YAHOO_DOMAINS.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
