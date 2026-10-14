//! FILA DE PERGUNTAS do overdev — quiz que a pessoa responde CLICANDO, sem interromper o run.
//!
//! O quê: perguntas que o agente enfileira com opções, contexto e o que a resposta destrava; e
//! a resposta da pessoa, com hora e revisão. Onde: `schematize overdev perguntar|responder|
//! revisar` no CLI, a aba Overdev da GUI, e o agente ao ler o que foi respondido.
//!
//! ## O que faltava, e por que o Markdown não aguentava
//!
//! O `resposta.rs` já sabe registrar `- [H r]` (respondeu, libera a máquina) e `- [H -]`
//! (recusou, cancela a máquina). O que ele não tem é **a pergunta**: só texto livre. O efeito
//! apareceu nesta sessão, medido: o agente perguntou *"`services` merece tela própria, **ou**
//! fica como lista secundária?"* e a resposta foi **"sim"** — que não responde um "ou". Uma
//! pergunta de duas pontas respondida com uma palavra é ambiguidade que nasce no formato, não
//! na pessoa.
//!
//! Markdown não guarda opção, auto-resposta, hora, nem "esta resposta ainda não foi revisada".
//! Guardar isso em prosa seria voltar a parsear texto humano — e este projeto já pagou duas
//! vezes por isso (a janela lendo rótulo em português, e a pílula vazia).
//!
//! ## Um arquivo por pergunta, e isto NÃO é detalhe
//!
//! A fila é `perguntas/<id>.json`, um arquivo por pergunta — **não** um `FILA.json` único. A
//! razão está escrita no vizinho [`super::caixa`], e vale igual aqui: um arquivo só teria dois
//! escritores (a GUI grava a resposta, o agente grava a revisão) num ciclo ler-modificar-
//! escrever. Quem escrever por último apaga o outro **sem erro e sem aviso**.
//!
//! Com um arquivo por pergunta, a GUI e o agente nunca disputam o mesmo caminho: cada escrita é
//! atômica e local a uma pergunta.
//!
//! ## `serde` aqui, e `saidajson.rs` à mão — a diferença é o leitor
//!
//! O `--json` dos apps é escrito à mão de propósito: é contrato com um leitor que está em OUTRO
//! repo, e mudar o shape tem de aparecer no diff. Aqui o leitor é `serde` nos dois lados, e o
//! mesmo `derive` garante que os dois concordem. O que mantém a propriedade do diff é
//! [`tests::o_shape_do_json_e_contrato`], que afirma **o JSON exato**: renomeie um campo e o teste
//! mostra a diferença. Guarantia igual, sem duas implementações para divergir.
//!
//! ## A resposta NÃO libera a máquina sozinha
//!
//! `resposta.revisada = false` deixa a pergunta em *respondida, aguardando revisão*. O agente tem
//! de LER a resposta e dizer se ela responde a pergunta feita. Foi pedido explicitamente — *"vc
//! tem que revisar minha resposta, não passar direto"* — e a sessão de 2026-09-24 mostrou por quê
//! com dois casos reais: o "sim" para um "ou", e um "sim, corte as tags" cujo pré-requisito estava
//! quebrado (os repos das janelas não existiam, e cortar publicaria release sem asset).

use super::trava::escreve_atomico;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// O tipo de pergunta — conjunto FECHADO.
///
/// **Por que fechado:** cada variante desenha um controle diferente na janela. Um `kind`
/// desconhecido tratado como "livre" desenharia uma pergunta sem os botões que ela precisa, e a
/// pessoa veria um campo de texto onde devia haver duas opções. Erro que se lê é melhor que UI
/// silenciosamente errada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Aprovar ou negar. Dois botões.
    Aprovar,
    /// Uma de N. Um botão por opção.
    Escolha,
    /// N de N. Caixas + confirmar.
    Multipla,
    /// Sim / Não / outro digitável.
    SimNaoOutro,
    /// Texto livre — o modal que já existe.
    Livre,
}

impl Kind {
    /// **O quê:** o slug estável do `kind` (o mesmo do `serde`: `sim_nao_outro`, …).
    ///
    /// **Onde:** o `--json` escrito à mão (`cli::saidajson`), que não passa pelo `derive`.
    pub fn slug(self) -> &'static str {
        match self {
            Kind::Aprovar => "aprovar",
            Kind::Escolha => "escolha",
            Kind::Multipla => "multipla",
            Kind::SimNaoOutro => "sim_nao_outro",
            Kind::Livre => "livre",
        }
    }

    /// **O quê:** este `kind` exige lista de opções?
    ///
    /// **Onde:** [`Pergunta::validar`]. `aprovar` e `sim_nao_outro` têm opções IMPLÍCITAS (são
    /// sempre as mesmas), então declará-las seria repetição que pode divergir.
    pub fn exige_opcoes(self) -> bool {
        matches!(self, Kind::Escolha | Kind::Multipla)
    }

    /// **O quê:** a resposta pode trazer texto livre?
    ///
    /// **Onde:** [`Pergunta::validar`]. Só `livre` (é todo texto) e `sim_nao_outro` (o "outro" é
    /// o texto). Em `aprovar`/`escolha`/`multipla` a resposta é o clique — texto ali é a
    /// ambiguidade de volta pela porta dos fundos.
    pub fn aceita_texto(self) -> bool {
        matches!(self, Kind::Livre | Kind::SimNaoOutro)
    }

    /// **O quê:** os ids que este `kind` aceita sem ninguém declarar.
    ///
    /// **Onde:** [`Pergunta::validar`] (entram no vocabulário de ids válidos) e
    /// [`Pergunta::resposta_em_texto`] (viram rótulo legível).
    ///
    /// **Por que implícitas e não declaradas:** aprovar/negar e sim/não são SEMPRE as mesmas.
    /// Obrigar cada pergunta a repeti-las abriria a porta para duas perguntas de aprovação com
    /// ids diferentes (`ok`/`sim`/`aprovo`), e a janela teria de adivinhar qual botão desenhar.
    pub fn opcoes_implicitas(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Kind::Aprovar => &[("aprovar", "Aprovar"), ("negar", "Negar")],
            Kind::SimNaoOutro => &[("sim", "Sim"), ("nao", "Não")],
            _ => &[],
        }
    }

    /// **O quê:** a resposta tem de conter pelo menos um id de opção?
    ///
    /// **Onde:** [`Pergunta::validar`]. `aprovar` entra: aprovar/negar são opções implícitas, e
    /// uma resposta sem nenhuma das duas não decidiu nada.
    pub fn exige_escolha(self) -> bool {
        matches!(self, Kind::Aprovar | Kind::Escolha | Kind::Multipla)
    }
}

/// Uma opção clicável.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Opcao {
    /// Id curto e estável — é o que a resposta grava. Nunca o rótulo: rótulo muda de redação.
    pub id: String,
    /// O texto do botão.
    pub rotulo: String,
    /// O que escolher isto significa. Vazio é aceitável para opção óbvia.
    #[serde(default)]
    pub detalhe: String,
}

/// A resposta da pessoa, e a revisão da máquina.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resposta {
    /// Epoch de QUANDO foi respondida — pedido explícito ("marcar quando eu respondi").
    pub em: u64,
    /// `humano` ou `auto` (a auto-resposta foi aceita por decurso, se algum dia houver isso).
    pub por: String,
    /// Ids de opção escolhidos. Só ids declarados em `opcoes`.
    #[serde(default)]
    pub escolhas: Vec<String>,
    /// Texto, quando o `kind` permite ("outro", ou `livre`).
    #[serde(default)]
    pub texto: String,
    /// A máquina já LEU esta resposta e concluiu algo? Enquanto `false`, nada é liberado.
    #[serde(default)]
    pub revisada: bool,
    /// O que a máquina concluiu ao revisar. Vazio enquanto não revisou.
    #[serde(default)]
    pub revisao: String,
}

/// Uma pergunta na fila.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pergunta {
    /// Id estável, nunca reusado. É o nome do arquivo e o vínculo com o checklist.
    pub id: String,
    /// Epoch de criação.
    pub criada_em: u64,
    /// A fase do checklist de onde ela saiu (`F5`, `FH`, …). Vazio quando não há.
    #[serde(default)]
    pub fase: String,
    /// A pergunta, numa linha.
    pub titulo: String,
    /// **Por que estou perguntando, e o que muda em cada caminho.** Pedido explícito
    /// ("o contexto para melhor ambientação"): a pessoa volta horas depois e precisa reconstruir
    /// o assunto sem reler o run inteiro.
    #[serde(default)]
    pub contexto: String,
    /// O tipo, que decide o controle na tela.
    pub kind: Kind,
    /// As opções, quando o `kind` as exige.
    #[serde(default)]
    pub opcoes: Vec<Opcao>,
    /// A sugestão do agente — id de opção. **Sugestão, nunca aplicada sozinha:** resposta que a
    /// pessoa não deu não é resposta dela.
    #[serde(default)]
    pub auto: String,
    /// Ids de itens de MÁQUINA no checklist que esta resposta destrava.
    #[serde(default)]
    pub linka: Vec<String>,
    /// A resposta, quando houver.
    #[serde(default)]
    pub resposta: Option<Resposta>,
}

/// O que há de errado com uma pergunta, se houver.
///
/// **Onde:** [`Pergunta::validar`], chamada ao gravar E ao ler. **Validar na LEITURA também**
/// porque o arquivo é editável à mão, e uma pergunta inválida no disco desenharia tela quebrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invalida {
    SemId,
    SemTitulo,
    /// `escolha`/`multipla` com menos de 2 opções — pergunta de uma opção não é pergunta.
    OpcoesInsuficientes(usize),
    /// Duas opções com o mesmo id: a resposta ficaria ambígua.
    OpcaoDuplicada(String),
    /// `auto` aponta opção que não existe.
    AutoDesconhecida(String),
    /// A resposta escolheu id que não está em `opcoes` — prosa disfarçada de decisão.
    EscolhaDesconhecida(String),
    /// `kind` sem opções recebeu opções: ou o kind está errado, ou as opções são ruído.
    OpcoesInesperadas,
    /// A resposta veio como TEXTO num `kind` que só aceita clique.
    ///
    /// **Este é o buraco que o teste de ponta a ponta achou em 2026-09-24.** As opções estavam
    /// declaradas, a validação de `escolhas` funcionava — e `--texto "sim"` passava por baixo,
    /// gravando exatamente a resposta ambígua que este formato existe para impedir. Invariante
    /// que cobre um campo e deixa o vizinho livre não cobre nada.
    TextoOndeSoCabeClique,
    /// A resposta não escolheu nada num `kind` que exige escolha.
    SemEscolha,
    /// `escolha` é UMA de N, e veio mais de uma.
    EscolhaMultiplaOndeCabeUma(usize),
}

impl std::fmt::Display for Invalida {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Invalida::SemId => write!(f, "pergunta sem id"),
            Invalida::SemTitulo => write!(f, "pergunta sem título"),
            Invalida::OpcoesInsuficientes(n) => {
                write!(f, "este kind exige 2 opções ou mais, e veio {n} — pergunta de uma opção não é pergunta")
            }
            Invalida::OpcaoDuplicada(id) => write!(f, "duas opções com o id `{id}`"),
            Invalida::AutoDesconhecida(id) => write!(f, "`auto` aponta `{id}`, que não é opção"),
            Invalida::EscolhaDesconhecida(id) => {
                write!(f, "a resposta escolheu `{id}`, que não é opção declarada")
            }
            Invalida::OpcoesInesperadas => {
                write!(f, "este kind não usa lista de opções — as dele são implícitas")
            }
            Invalida::TextoOndeSoCabeClique => write!(
                f,
                "este kind se responde CLICANDO numa opção, e veio texto livre — texto aqui é \
                 a ambiguidade que o formato existe para impedir (use `sim_nao_outro` ou \
                 `livre` se a resposta é aberta)"
            ),
            Invalida::SemEscolha => {
                write!(f, "este kind exige escolher ao menos uma opção, e nenhuma veio")
            }
            Invalida::EscolhaMultiplaOndeCabeUma(n) => write!(
                f,
                "`escolha` é UMA de N, e vieram {n} — use `multipla` se mais de uma é válida"
            ),
        }
    }
}

impl Pergunta {
    /// **O quê:** as invariantes do formato. Vazio = válida.
    ///
    /// **Onde:** [`gravar`] e [`ler`]. Devolve TODOS os problemas, não o primeiro: quem está
    /// consertando um arquivo à mão quer a lista, não uma descoberta por vez.
    pub fn validar(&self) -> Vec<Invalida> {
        let mut e = Vec::new();
        if self.id.trim().is_empty() {
            e.push(Invalida::SemId);
        }
        if self.titulo.trim().is_empty() {
            e.push(Invalida::SemTitulo);
        }
        if self.kind.exige_opcoes() {
            if self.opcoes.len() < 2 {
                e.push(Invalida::OpcoesInsuficientes(self.opcoes.len()));
            }
        } else if !self.opcoes.is_empty() {
            e.push(Invalida::OpcoesInesperadas);
        }
        let mut vistos: Vec<&str> =
            self.kind.opcoes_implicitas().iter().map(|(id, _)| *id).collect();
        for o in &self.opcoes {
            if vistos.contains(&o.id.as_str()) {
                e.push(Invalida::OpcaoDuplicada(o.id.clone()));
            }
            vistos.push(&o.id);
        }
        if !self.auto.is_empty() && !vistos.contains(&self.auto.as_str()) {
            e.push(Invalida::AutoDesconhecida(self.auto.clone()));
        }
        if let Some(r) = &self.resposta {
            for c in &r.escolhas {
                if !vistos.contains(&c.as_str()) {
                    e.push(Invalida::EscolhaDesconhecida(c.clone()));
                }
            }
            // O texto é permitido SÓ onde a resposta é legitimamente aberta.
            if !r.texto.trim().is_empty() && !self.kind.aceita_texto() {
                e.push(Invalida::TextoOndeSoCabeClique);
            }
            if self.kind.exige_escolha() {
                match r.escolhas.len() {
                    0 => e.push(Invalida::SemEscolha),
                    n if n > 1 && self.kind == Kind::Escolha => {
                        e.push(Invalida::EscolhaMultiplaOndeCabeUma(n))
                    }
                    _ => {}
                }
            }
        }
        e
    }

    /// **O quê:** esta pergunta está esperando alguém? `true` enquanto não houver resposta.
    ///
    /// **Onde:** a listagem da GUI e do CLI.
    pub fn aberta(&self) -> bool {
        self.resposta.is_none()
    }

    /// **O quê:** respondida, mas a máquina ainda não leu. **Onde:** o gate e o agente.
    ///
    /// **Por que é um estado próprio:** é aqui que o pedido "não passar direto" vive. Contar isto
    /// como fechado deixaria o agente agir sobre uma resposta que ninguém conferiu — e o "sim"
    /// para uma pergunta de duas pontas viraria uma decisão inventada.
    pub fn aguarda_revisao(&self) -> bool {
        self.resposta.as_ref().is_some_and(|r| !r.revisada)
    }

    /// **O quê:** a resposta em uma linha legível, para o log e para a revisão do agente.
    ///
    /// **Onde:** `overdev perguntas` no CLI.
    pub fn resposta_em_texto(&self) -> String {
        let Some(r) = &self.resposta else { return "(sem resposta)".into() };
        let mut partes: Vec<String> = r
            .escolhas
            .iter()
            .map(|id| {
                self.opcoes
                    .iter()
                    .find(|o| &o.id == id)
                    .map(|o| o.rotulo.clone())
                    .or_else(|| {
                        self.kind
                            .opcoes_implicitas()
                            .iter()
                            .find(|(i, _)| i == id)
                            .map(|(_, r)| (*r).to_string())
                    })
                    .unwrap_or_else(|| id.clone())
            })
            .collect();
        if !r.texto.trim().is_empty() {
            partes.push(r.texto.trim().to_string());
        }
        if partes.is_empty() {
            "(resposta vazia)".into()
        } else {
            partes.join(" · ")
        }
    }
}

/// `<overdev>/perguntas`
pub fn dir_perguntas(root: &Path) -> PathBuf {
    crate::paths::overdev_dir_at(root).join("perguntas")
}

/// **O quê:** o caminho do arquivo de uma pergunta.
///
/// **Onde:** [`gravar`], [`ler`]. O id é sanitizado porque ele vira nome de arquivo, e um id com
/// `/` ou `..` escreveria fora do diretório.
pub fn caminho(root: &Path, id: &str) -> PathBuf {
    let seguro: String =
        id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    dir_perguntas(root).join(format!("{seguro}.json"))
}

/// **O quê:** grava a pergunta, recusando a inválida.
///
/// **Onde:** `overdev perguntar` (o agente), `overdev responder` (a GUI/CLI).
///
/// **Recusar na gravação é o ponto:** validar só na leitura deixaria o arquivo ruim no disco, e
/// quem o gravou já teria ido embora. O erro tem de chegar em quem causou.
pub fn gravar(root: &Path, p: &Pergunta) -> Result<PathBuf, String> {
    let erros = p.validar();
    if !erros.is_empty() {
        let lista: Vec<String> = erros.iter().map(|e| e.to_string()).collect();
        return Err(format!("pergunta inválida: {}", lista.join("; ")));
    }
    let alvo = caminho(root, &p.id);
    if let Some(d) = alvo.parent() {
        std::fs::create_dir_all(d)
            .map_err(|e| format!("não consegui criar {}: {e}", d.display()))?;
    }
    let json = serde_json::to_string_pretty(p).map_err(|e| format!("serializando: {e}"))?;
    escreve_atomico(&alvo, &json).map_err(|e| format!("gravando {}: {e}", alvo.display()))?;
    Ok(alvo)
}

/// **O quê:** lê uma pergunta do disco, recusando a inválida e a ilegível.
///
/// **Onde:** `overdev responder|revisar|perguntas`, e a GUI.
///
/// **Por que valida na leitura também:** o arquivo é editável à mão (é JSON, e a pessoa vai
/// editar). Uma pergunta com `auto` apontando opção inexistente desenharia uma sugestão
/// fantasma; é melhor dizer o que está errado do que desenhar errado.
pub fn ler(caminho: &Path) -> Result<Pergunta, String> {
    let bruto = std::fs::read_to_string(caminho)
        .map_err(|e| format!("lendo {}: {e}", caminho.display()))?;
    let p: Pergunta = serde_json::from_str(&bruto)
        .map_err(|e| format!("{} não é uma pergunta válida: {e}", caminho.display()))?;
    let erros = p.validar();
    if !erros.is_empty() {
        let lista: Vec<String> = erros.iter().map(|e| e.to_string()).collect();
        return Err(format!("{}: {}", caminho.display(), lista.join("; ")));
    }
    Ok(p)
}

/// **O quê:** todas as perguntas, mais novas primeiro, com os arquivos ruins SEPARADOS.
///
/// **Onde:** `overdev perguntas`, a aba Overdev da GUI, e o gate.
///
/// **Por que devolve os erros em vez de pulá-los:** um arquivo ilegível pulado em silêncio é uma
/// pergunta que desapareceu da fila — o usuário acha que respondeu tudo, e havia uma esperando.
pub fn listar(root: &Path) -> (Vec<Pergunta>, Vec<String>) {
    let dir = dir_perguntas(root);
    let mut ok = Vec::new();
    let mut ruins = Vec::new();
    let Ok(rd) = std::fs::read_dir(&dir) else { return (ok, ruins) };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "json") {
            match ler(&p) {
                Ok(q) => ok.push(q),
                Err(msg) => ruins.push(msg),
            }
        }
    }
    ok.sort_by_key(|p| std::cmp::Reverse(p.criada_em));
    ruins.sort();
    (ok, ruins)
}

/// **O quê:** um id novo, estável e sem colisão prática.
///
/// **Onde:** `overdev perguntar`. Epoch em base 36 + contador do diretório: legível, ordenável e
/// curto o bastante para caber num marcador de checklist.
pub fn novo_id(root: &Path, agora: u64) -> String {
    let n = std::fs::read_dir(dir_perguntas(root)).map(|d| d.count()).unwrap_or(0);
    format!("q-{}-{}", base36(agora), n + 1)
}

fn base36(mut n: u64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut s = Vec::new();
    while n > 0 {
        s.push(D[(n % 36) as usize]);
        n /= 36;
    }
    s.reverse();
    String::from_utf8(s).expect("base36 é ASCII")
}
#[cfg(test)]
#[path = "pergunta_testes.rs"]
mod tests;
