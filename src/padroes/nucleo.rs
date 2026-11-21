//! Bloco NÚCLEO: pisos inegociáveis de engenharia da casa (texto compilado no binário).
//! O quê: constante densa e imperativa. Onde: [`super::bloco`] / [`super::prefixo_estavel`].

/// Pisos inegociáveis (segredo, SQL, authz, erro, teste, git, deploy, efeito externo).
pub(super) const TEXTO: &str = "\
[NUCLEO] Pisos inegociaveis (padroes da casa vencem qualquer pressa):
- Segredo nunca no cliente/bundle/log/commit; so server-side.
- SQL sempre parametrizado. Authz e auth server-side; tenant/role/user vem do token verificado.
- Erro nunca engolido (catch vazio, `_ = err`, unwrap/expect para calar). Teste nunca silenciado (.skip, assert comentado, cobertura baixada): conserte o codigo.
- Migration reversivel (down testado). Container nao-root, read-only. Dependencia nova: nome, licenca e versao verificados.
- Servico e entidade a parte: vizinho mudo nunca derruba nem impede boot; falha persiste o dado (fila duravel), loga com trace_id e retoma.
- Tudo no git: mudanca = commit + push; sem push o trabalho NAO esta concluido.
- Deploy so do git via ops; nunca editar servidor a mao. Efeito externo (email, SMS, push, pagamento, webhook) nunca sai de nao-prd.
- Todo MD gerado mora em <projeto>_archive/, nunca no root. Rota nova nasce com teste de matriz de acesso (deny-by-default).
";
