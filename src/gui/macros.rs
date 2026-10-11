//! O macro que liga um callback do global `Ov` à janela — o resto das telas o usa.
//!
//! **O quê:** `com_ov!(w, e, on_xxx, |w, e, args…| { corpo })` registra o callback segurando a
//! janela por referência FRACA (`upgrade` no clique) e o `Estado` por `Rc`.
//!
//! **Onde:** [`super::resolver`], [`super::execucao`], [`super::caixa`] e [`super::notas`]. Fica
//! num arquivo à parte, declarado ANTES dos módulos que o usam (macro de `macro_rules!` só vale
//! para o que vem depois dele no texto).
//!
//! **Por que fraca:** o callback vive dentro da própria janela; segurá-la forte seria um ciclo
//! que nunca libera.

macro_rules! com_ov {
    ($w:expr, $e:expr, $registrar:ident, |$jw:ident, $je:ident $(, $a:ident)*| $corpo:block) => {{
        let weak = $w.as_weak();
        let est = $e.clone();
        $w.global::<crate::Ov>().$registrar(move |$($a),*| {
            if let Some($jw) = weak.upgrade() {
                let $je = &est;
                $corpo
            }
        });
    }};
}
