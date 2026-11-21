//! Bloco SEGURANÇA: supply chain e entrada hostil.
//! O quê: constante densa. Onde: [`super::bloco`] / [`super::prefixo_estavel`].

/// Supply chain, entrada hostil, segredos e logs.
pub(super) const TEXTO: &str = "\
[SEGURANCA] Supply chain e entrada hostil:
- Dependencia: lockfile commitado, versao pinada, licenca e mantenedor verificados, auditoria (cargo audit/deny); sem script de instalacao remoto executado as cegas.
- Toda entrada e hostil: valide tipo, tamanho e encoding; nunca 500 por input malformado, nunca coercao de tipo, nunca eco sem escape.
- Artefato escrito por unidade jaulada e lido por componente de confianca e entrada hostil (validar, copiar para area neutra, sem execucao automatica).
- Segredo fora de log, prompt e commit; token por CSPRNG; hash de senha argon2id/bcrypt>=12.
";
