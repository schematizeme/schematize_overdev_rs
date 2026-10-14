//! Análise de um checklist monolítico em seções e blocos (insumo do `particionar`).
//!
//! Um BLOCO é um item de topo (`- [..] …`) com tudo que o acompanha (subitens indentados,
//! notas, linhas em branco). Cortar só entre blocos garante que nenhuma linha se perde nem
//! se duplica. Cabeçalhos `###` e a prosa que os segue viram "lead" do bloco seguinte.

use super::estado::Estado;

/// Um item de topo com seu acompanhamento.
#[derive(Debug, Clone, Default)]
pub struct Bloco {
    /// Linhas do bloco, na ordem original (sem `\n`).
    pub linhas: Vec<String>,
    /// Itens contados no bloco (inclui subitens indentados).
    pub n: usize,
    /// O bloco abre um grupo `###`.
    pub h3: bool,
}

/// Uma seção `##` (ou o preâmbulo com itens, ou um arquivo legado inteiro).
#[derive(Debug, Clone, Default)]
pub struct Secao {
    /// Linha `## …` original (vazia para preâmbulo/legado).
    pub cabecalho: String,
    /// Título limpo, sem a tag `[pN]`.
    pub titulo: String,
    /// Prioridade pela tag `[p0]..[p3]` do título; `None` = default `p2`.
    pub prio: Option<u8>,
    /// Blocos de itens da seção.
    pub blocos: Vec<Bloco>,
    /// Linhas soltas sem item nenhum depois (prosa de seção sem itens).
    pub sobra: Vec<String>,
}

impl Secao {
    /// **O quê:** total de itens da seção. **Onde:** `fatiar` e a fusão de seções pequenas.
    pub fn total(&self) -> usize {
        self.blocos.iter().map(|b| b.n).sum()
    }
}

/// **O quê:** a linha é um item reconhecido (qualquer marcador do `count_str`), indentado ou não.
/// **Onde:** [`blocos_de`] e os testes de integridade.
pub fn eh_item(linha: &str) -> bool {
    Estado::de(linha).total() > 0
}

/// **O quê:** a linha é item de TOPO (sem indentação). **Onde:** [`blocos_de`].
fn eh_item_topo(linha: &str) -> bool {
    linha.starts_with("- [") && eh_item(linha)
}

/// **O quê:** divide `texto` em linhas preservando `\r` (byte a byte); sem a "linha" vazia final.
/// **Onde:** [`analisar`] e `migrar`.
pub fn linhas_de(texto: &str) -> Vec<String> {
    let mut v: Vec<String> = texto.split('\n').map(String::from).collect();
    if v.last().is_some_and(String::is_empty) {
        v.pop();
    }
    v
}

/// **O quê:** extrai a tag `[p0]..[p3]` de um título; devolve `(título sem a tag, prio)`.
/// **Onde:** [`analisar`].
pub fn tira_tag(titulo: &str) -> (String, Option<u8>) {
    let b = titulo.as_bytes();
    for i in 0..b.len().saturating_sub(3) {
        if b[i] == b'[' && b[i + 1] == b'p' && (b'0'..=b'3').contains(&b[i + 2]) && b[i + 3] == b']'
        {
            let limpo = format!("{}{}", &titulo[..i], &titulo[i + 4..]);
            return (limpo.split_whitespace().collect::<Vec<_>>().join(" "), Some(b[i + 2] - b'0'));
        }
    }
    (titulo.trim().to_string(), None)
}

/// **O quê:** agrupa as linhas de uma seção em blocos; o que sobra sem item vai para `sobra`.
/// **Onde:** [`analisar`].
pub fn blocos_de(corpo: &[String]) -> (Vec<Bloco>, Vec<String>) {
    let mut blocos: Vec<Bloco> = Vec::new();
    let mut lead: Vec<String> = Vec::new();
    let mut em_lead = true;
    for l in corpo {
        if eh_item_topo(l) {
            let mut linhas = std::mem::take(&mut lead);
            let h3 = linhas.iter().any(|x| x.starts_with("### "));
            linhas.push(l.clone());
            blocos.push(Bloco { linhas, n: 0, h3 });
            em_lead = false;
        } else if !eh_item(l) && l.starts_with('#') {
            em_lead = true;
            lead.push(l.clone());
        } else if em_lead {
            lead.push(l.clone());
        } else if let Some(b) = blocos.last_mut() {
            b.linhas.push(l.clone());
        }
    }
    let mut sobra = Vec::new();
    if blocos.is_empty() {
        sobra = lead;
    } else if let Some(b) = blocos.last_mut() {
        b.linhas.append(&mut lead);
    }
    for b in &mut blocos {
        b.n = Estado::de(&b.linhas.join("\n")).total();
    }
    (blocos, sobra)
}

/// Resultado de [`analisar`]: prosa que fica no índice + seções com itens.
#[derive(Debug, Default)]
pub struct Analise {
    /// Linhas do preâmbulo anteriores ao 1º item (vão para o índice).
    pub prosa: Vec<String>,
    /// Seções (a primeira pode ser o preâmbulo com itens, título `geral`).
    pub secoes: Vec<Secao>,
}

/// **O quê:** analisa um checklist monolítico: preâmbulo, seções `##`, blocos. Seção sem item
/// vira prosa do índice (nada se perde). **Onde:** `migrar::particionar`.
pub fn analisar(texto: &str, titulo_preambulo: &str) -> Analise {
    let linhas = linhas_de(texto);
    let mut a = Analise::default();
    let primeiro_h2 = linhas.iter().position(|l| l.starts_with("## ")).unwrap_or(linhas.len());
    let pre = &linhas[..primeiro_h2];
    let corte = pre.iter().position(|l| eh_item(l)).unwrap_or(pre.len());
    a.prosa.extend(pre[..corte].iter().cloned());
    if corte < pre.len() {
        let (blocos, _) = blocos_de(&pre[corte..]);
        a.secoes.push(Secao { titulo: titulo_preambulo.into(), blocos, ..Secao::default() });
    }
    let mut i = primeiro_h2;
    while i < linhas.len() {
        let fim = linhas[i + 1..]
            .iter()
            .position(|l| l.starts_with("## "))
            .map_or(linhas.len(), |p| i + 1 + p);
        let (titulo, prio) = tira_tag(linhas[i].trim_start_matches('#').trim());
        let (blocos, sobra) = blocos_de(&linhas[i + 1..fim]);
        if blocos.is_empty() {
            a.prosa.push(linhas[i].clone());
            a.prosa.extend(sobra);
        } else {
            a.secoes.push(Secao { cabecalho: linhas[i].clone(), titulo, prio, blocos, sobra });
        }
        i = fim;
    }
    a
}

/// **O quê:** fatia os blocos de uma seção em grupos de ≤`max` itens, de preferência cortando
/// nas fronteiras `###`; sem `###` (ou grupo maior que `max`) corta entre itens, equilibrado.
/// **Onde:** `migrar::particionar`. Nunca round-robin; ordem original preservada.
pub fn fatiar(blocos: &[Bloco], max: usize) -> Vec<Vec<Bloco>> {
    let total: usize = blocos.iter().map(|b| b.n).sum();
    if total <= max {
        return vec![blocos.to_vec()];
    }
    let k = total.div_ceil(max);
    let alvo = total.div_ceil(k);
    let tamanho_grupo = |i: usize| -> usize {
        blocos[i..].iter().enumerate().take_while(|(j, b)| *j == 0 || !b.h3).map(|(_, b)| b.n).sum()
    };
    let mut saida: Vec<Vec<Bloco>> = Vec::new();
    let mut cur: Vec<Bloco> = Vec::new();
    let mut cur_n = 0usize;
    let mut grupo_grande = false;
    for (i, b) in blocos.iter().enumerate() {
        if b.h3 || i == 0 {
            let g = tamanho_grupo(i);
            grupo_grande = g > max || (i == 0 && !blocos.iter().any(|x| x.h3));
            if cur_n > 0 && (cur_n + g > max || cur_n >= alvo) {
                saida.push(std::mem::take(&mut cur));
                cur_n = 0;
            }
        }
        let estoura = cur_n + b.n > max || (grupo_grande && cur_n > 0 && cur_n + b.n > alvo);
        if cur_n > 0 && estoura {
            saida.push(std::mem::take(&mut cur));
            cur_n = 0;
        }
        cur_n += b.n;
        cur.push(b.clone());
    }
    if !cur.is_empty() {
        saida.push(cur);
    }
    saida
}
