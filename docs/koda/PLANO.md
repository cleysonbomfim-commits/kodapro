# Plano do KODA Pro

> Atualizado em 2026-10-15. O plano é do dono do projeto; ajuste quando mudar de ideia.

O KODA Pro é um fork do [FilmCraft](https://github.com/storytold/filmcraft), editor de vídeo não
linear open-source em Rust puro (código MIT OR Apache-2.0). O fork vive em
[cleysonbomfim-commits/kodapro](https://github.com/cleysonbomfim-commits/kodapro); o original está no
remote `upstream`.

## Leitura obrigatória no início de cada sessão

1. Este arquivo (`docs/koda/PLANO.md`) e o [diário](DIARIO.md).
2. As regras do upstream continuam valendo: [`AGENTS.md`](../../AGENTS.md) (nunca travar, nada da
   Adobe, código feito do zero, só dependências com licença permissiva) e [`CLAUDE.md`](../../CLAUDE.md).

## Princípios

- **Fork do FilmCraft, puxando atualizações do upstream.** Mexer o mínimo possível no código
  original, para que `git merge upstream/main` continue simples.
- **Nossas coisas em módulos separados.** Funcionalidades do KODA vivem em crates próprios
  (`crates/koda-*`, por exemplo `koda-color`, `koda-motion`, `koda-ai`) e numa pasta própria da
  interface; o código original só ganha os "ganchos" necessários (registrar um painel, um comando,
  uma aba). Cada crate novo entra na tabela de camadas (`xtask/src/main.rs`, `LAYERS`) para passar
  em `cargo xtask layers`.
- **O que o usuário vê é KODA; o que é interno continua FilmCraft.** Nome, ícone e marca visíveis
  são do KODA Pro; crates, binário `filmcraft`, ids de comandos, `.fcproj` e pasta de dados ficam
  como estão.
- **Licenças.** O copyright do FilmCraft, `LICENSE-*` e `NOTICE` ficam. A marca ArtCraft foi removida
  (exigência da licença de marca para forks). O ícone K é marca do dono, não open source.
- **Uma etapa por vez:** cada etapa concluída ganha entrada no diário, commit e `git push`.

## Já feito (preparação)

- Clone, remote `upstream`, compilação em release no Mac.
- Nome exibido trocado para Koda Pro; marca ArtCraft removida; ícone K; `Koda Pro.app` instalável
  com `packaging/macos/koda-app.sh --install`.
- Diário e este plano em `docs/koda/`, lidos no início de cada sessão (aviso no `CLAUDE.md`).

Ainda pendente da preparação (entra na etapa 1): rodar o portão completo (`cargo xtask ci`) e criar
a rotina de merge periódico do `upstream`.

## Etapas

| # | Etapa | Estado |
|---|---|---|
| 1 | Base funcionando no Mac com os arquivos da Sony FX30 | a fazer |
| 2 | Personalização: atalhos, layout e visual KODA | a fazer |
| 3 | Aba de cor por nós, estilo DaVinci Resolve | a fazer |
| 4 | Aba Motion, estilo After Effects / Fusion | a fazer |
| 5 | Ferramentas de IA | a fazer |

### 1. Base funcionando no Mac com os arquivos da Sony FX30

Objetivo: importar, editar, tocar e exportar os arquivos da FX30 sem travar.

- A FX30 grava XAVC S (H.264) e XAVC HS (H.265), em 8 ou 10 bits, 4:2:0 ou 4:2:2, além de XAVC S-I
  (intra), muitas vezes em S-Log3 / S-Gamut3.Cine, com áudio PCM em MP4.
- Hoje o decodificador próprio do FilmCraft só faz H.264 8-bit 4:2:0 e HEVC sem 4:2:2
  ([gaps.md G3](../gaps.md#g3-10-bit-and-422-camera-media-needs-hardware)). No Mac o VideoToolbox
  decodifica 10-bit 4:2:2 em chips que suportam, então o primeiro passo é medir o que já funciona.
- Passos: juntar arquivos de teste reais de cada modo de gravação; testar importação, reprodução
  fluida, cortes, Lumetri com LUT de S-Log3 e exportação; corrigir o que falhar (de preferência no
  decodificador via hardware e, se preciso, contribuindo de volta ao upstream).
- Pronto quando: todos os modos de gravação usados pelo dono abrem, tocam em tempo real e exportam
  certo, e o portão `cargo xtask ci` passa.

### 2. Personalização: atalhos, layout e visual KODA

- Preset de atalhos KODA (o FilmCraft já tem presets em `crates/engine/src/shortcut_presets.rs`).
- Workspace (layout de painéis) padrão do KODA.
- Tema visual KODA (cores, tipografia, preto e branco do ícone) como tema próprio, sem reescrever
  o tema original.

### 3. Aba de cor por nós, estilo DaVinci Resolve

- Nova aba "Cor" com um grafo de nós (serial, paralelo, layer), cada nó com correções primárias
  (rodas, curvas), secundárias (qualificador, janelas/máscaras) e LUTs.
- Reaproveitar a matemática de cor do FilmCraft (`filmcraft-color`, Lumetri, scopes) e guardar o
  grafo como efeito no clipe, para que reprodução e exportação funcionem sem mudar o núcleo.
- Crate próprio: `koda-color` (modelo do grafo e avaliação) + painel na interface.

### 4. Aba Motion, estilo After Effects / Fusion

- Composição por camadas ou nós com keyframes, máscaras, textos e formas, usando o motor de
  keyframes, máscaras e gráficos que já existe.
- Crate próprio: `koda-motion`.

### 5. Ferramentas de IA

- Candidatas: transcrição e legendas (o FilmCraft já tem Whisper local), recorte de silêncios,
  sugestões de corte, remoção de fundo, correção de cor automática.
- Modelos só com licença que permita uso e distribuição; rodando localmente sempre que possível.
- Crate próprio: `koda-ai`.

## Como compilar e abrir

```sh
packaging/macos/koda-app.sh --install       # compila e instala ~/Applications/Koda Pro.app
./target/release/filmcraft --control 9876   # ou direto pelo terminal, com o canal de controle
```
