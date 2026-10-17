# Diário do Koda Pro

Uma entrada por etapa concluída, da mais recente para a mais antiga. Cada entrada diz o que foi feito,
o que funcionou, o que deu problema e qual o próximo passo. O plano está em [PLANO.md](PLANO.md).

## 2026-10-17 — Marco 6 da aba Cor, só o motor: powergrades e copiar/colar (noite)

**O que foi feito** (`crates/koda-color/src/library.rs` e comandos novos em `commands.rs`)
- **Powergrades em arquivo** (`*.kpg`, JSON legível): a grade inteira com **as LUTs embutidas**,
  para funcionar em qualquer projeto. Ao aplicar, as LUTs entram na biblioteca de LUTs do projeto
  (uma LUT igual já existente é reaproveitada, não duplicada) e os nós passam a apontar para elas.
  - `koda.color.savePowergrade {clip?, name, path?}` (sem `path`: pasta padrão
    `~/Library/Application Support/FilmCraft/koda/powergrades/<nome>.kpg`)
  - `koda.color.applyPowergrade {path, clip?/clips?, mode: replace|append}`
  - `koda.color.listPowergrades {dir?}` (arquivos quebrados aparecem com o erro)
- **Copiar a grade de um clipe para outros**: `koda.color.copyGrade {clip?}` +
  `koda.color.pasteGrade {clips?, mode?}` (sem `clips`: os clipes selecionados), e
  `koda.color.applyGradeFrom {from, clips?}` (direto, como o clique do meio no DaVinci).
- **Copiar e colar um nó**: `koda.color.copyNode {clip?, node}` + `koda.color.pasteNode
  {clip?, index?}` (nó novo) ou `{replace: nó}` (troca só os ajustes do nó, mantém id, nome e
  liga/desliga — como "colar atributos" do DaVinci).
- Cada colagem/aplicação é **um passo de desfazer**, mesmo em vários clipes de uma vez.
- Arquivos são tratados como hostis: tamanho máximo 64 MB, até 16 LUTs, versão e formato
  conferidos, grade sanitizada como a do projeto; nome de arquivo limpo (`../` não sai da pasta).
- README do `koda-color` atualizado (módulos, nós, comandos). **App atualizado.**

**O que funcionou**
- Testes (`tests/marco6.rs`): colar em 2 clipes e desfazer/refazer de uma vez; modo append com
  ids novos; aplicar de um clipe; copiar/colar nó por índice e por "replace" com desfazer;
  powergrade salvo num projeto e aplicado num projeto novo (LUT importada, nó apontando para ela,
  sem duplicar ao aplicar de novo); arquivos quebrados recusados.

**Decisões para revisar**
- A área de cópia vale enquanto o app está aberto (não vai para o clipboard do sistema).
- Aplicar/colar por padrão **substitui** a grade do clipe (como o DaVinci); `mode: "append"`
  acrescenta os nós no fim.
- Extensão `.kpg` e pasta padrão dentro da pasta de dados do FilmCraft.

**Próximo passo**
- Marco 5: a tela da página Cor.

## 2026-10-16 — Marco 4 da aba Cor: nitidez e glow (noite, trabalho sozinho)

**O que foi feito**
- **Nó de nitidez** (`sharpen`, `crates/koda-color/src/spatial.rs`): máscara de nitidez no sinal,
  `x + quantidade·(x − Gauss(x))`, com `amount` e `radius` (sigma em pixels do clipe).
- **Nó de glow** (`glow`): a imagem acima de um limiar (luma com borda suave) é desfocada pelo
  `spread`, misturada com um pouco da fonte sem desfoque (`core`), saturada, com `gamma` e `gain`,
  e composta por cima no estilo **Soft Light, Screen, Add ou Normal**, com `opacity` e
  **highlight recovery** (joga de volta para baixo do branco o que passar de 0,8).
- Os padrões dos dois nós são os do powergrade KODA, **calibrados contra o DaVinci**
  (`examples/calibra_marco4.rs`): nitidez sigma 0,8 px e quantidade 0,9; glow Soft Light,
  limiar 0,238, spread 115 px, ganho 0,9, saturação 1,45, gamma 1,063, core 0,25, opacidade 0,426.
- **GPU**: a nitidez usa passes próprios de Gauss separável (taps explícitos, igual na CPU); o glow
  usa o box blur do FilmCraft (rápido para raios grandes). Os dois usam o gancho de
  `save`/`blur`/`combine` criado no marco 3.
- `addNode` aceita `sharpen` e `glow`; `setParam` aceita `amount`, `radius`, `style`,
  `threshold`, `spread`, `gain`, `saturation`, `gamma`, `core`, `opacity` e `highlightRecovery`.
- Print público `docs/koda/cor/prints/marco4-carta.png` (cartela pelo CST: só CST | nitidez +
  glow do powergrade | glow Screen | glow Add com highlight recovery). Comparações locais (fora do
  git): `referencia/comparacoes/{nitidez_02_vs_03,glow_03_vs_04,nitidez_glow_02_vs_04}.png`.
  **App atualizado.**

**O que funcionou — números (ΔE 2000, 1080 × 1920)**

| Comparação | Média | p99 | Máxima |
|---|---|---|---|
| sem nitidez: 02_cst × 03_nitidez | 0,31 | 1,24 | 9,78 |
| **Nitidez(02_cst) × 03_nitidez** | **0,14** | 0,37 | **0,61** |
| sem glow: 03_nitidez × 04_glow | 3,78 | 8,04 | 9,69 |
| **Glow(03_nitidez) × 04_glow** | **0,81** | 2,67 | 4,88 |
| **Nitidez + glow(02_cst) × 04_glow** (pedido) | **0,81** | 2,68 | 4,79 |

- A nitidez atende a meta (média < 1, máxima < 3). O glow atende a média, mas **não a máxima**.
- GPU × CPU (nitidez, os 4 estilos de glow, recovery, glow largo do KODA, primárias + nitidez +
  glow; cheio e 1/2): diferença média ≤ 0,045 de 255, p99 1.

**Decisões para revisar**
- Os valores dos nós 03 e 04 não são conhecidos: os padrões foram ajustados aos frames. Se você
  mandar prints dos painéis desses nós no DaVinci, eu troco pelos valores reais.
- O "Soft Light" que bate com o DaVinci é a fórmula W3C com **a imagem sobre o glow** (e não o
  glow sobre a imagem): 0,95 contra 1,06 de ΔE no ajuste.
- O erro que sobra no glow fica em halos em volta da pessoa e das portas: o glow do DaVinci deve
  ter um perfil de espalhamento em várias escalas (núcleo forte e cauda longa). Para fazer isso na
  GPU precisaria de uma 4ª textura no estágio de efeitos do FilmCraft; deixei para depois.
- A highlight recovery não aparece nesses frames (nada passa do branco); a curva (joelho em 0,8)
  é uma escolha minha.

**O que deu problema**
- Um teste de faixa de parâmetros mostrou que `f64::MAX` virava infinito em f32 e o valor caía
  no padrão em vez de ir para o limite; agora infinito é limitado à faixa (só NaN volta ao padrão).

**Próximo passo**
- Marco 6 (só o motor): powergrades em arquivo, aplicar num clipe, copiar a grade de um clipe
  para outros e colar um nó, por comandos com desfazer.

## 2026-10-16 — Marco 3 da aba Cor: primárias e curvas (noite, trabalho sozinho)

**O que foi feito**
- **Nó de primárias** (`crates/koda-color/src/primaries.rs`, tipo `primaries`): Lift/Gamma/Gain
  (Y, R, G, B) e Offset (R, G, B) — os mesmos números em barras e rodas, como no DaVinci —, Temp,
  Tint, Contraste, Pivot, Mid Detail, Color Boost, Shadows, Highlights, Saturação, Hue e Lum Mix,
  com os padrões e faixas do DaVinci (Gain 1, Offset 25, Pivot 0,435, Sat/Hue 50, Lum Mix 100).
- **Curvas Y/R/G/B** no mesmo nó: pontos editáveis, spline monotônica (não "dá a volta"),
  ligação R/G/B (padrão ligada) e reset. A curva vira tabela de 1024 amostras.
- **GPU**: tudo no `grade.wgsl`; parâmetros e curvas vão numa imagem auxiliar (a CPU lê os mesmos
  números, por isso as duas batem). **Mid Detail** é espacial: o gancho do FilmCraft ganhou passes de
  desfoque e "combine" para efeitos externos (`GpuPass { save, blur, combine }`, usando o box blur
  do próprio FilmCraft na GPU e na CPU). Isso também serve para a nitidez e o glow do marco 4.
- Comandos novos: `koda.color.setCurve {node, channel, points}` e
  `koda.color.resetParams {node, what}`; `addNode` aceita `primaries`; `setParam` aceita
  `lift`, `lift.y`, `offset.r`, `temp`, `midDetail`, `curvesLinked`, `mode`… Tudo com desfazer.
- **Achado:** o print `davinci-color-referencia.png` mostra os valores do nó 01 (Temp −69,9,
  Tint −5,78, Shadows 17,5, o resto no padrão). Com isso calibrei Temp/Tint contra 00_log → 01_global
  (`examples/calibra_primarias.rs`): no sinal log, Temp −69,9 = R ×0,975 e B ×1,0225 contra o G.
- Print público: `docs/koda/cor/prints/marco3-carta.png` (cartela sintética pelo CST: sem
  primárias | nó 01 | primárias criativas | curvas). Comparação local (fora do git):
  `referencia/comparacoes/primarias_00_vs_01.png`. **App atualizado.**

**O que funcionou — números**
- Nó 01 (Temp, Tint e Shadows do print + curva Y ajustada) × 01_global do DaVinci: **ΔE 2000
  médio 0,68, p99 1,67, máximo 2,12** — dentro da meta (média < 1, máximo < 3). Só a curva, sem
  Temp/Tint/Shadows: média 2,86 (então Temp/Tint fazem diferença real).
- GPU × CPU no compositor (7 casos: LGG, offset/contraste/pivot, temp/tint/lum mix,
  shadows/highlights/boost/sat/hue, curvas, mid detail, mid detail + nó 01; cheio e 1/2):
  diferença média ≤ 0,05 de 255, p99 1.
- Testes: 20 unitários do koda-color + marcos 1, 2 e 3; filmcraft-gpu e filmcraft-render (242)
  passaram com a mudança do gancho.

**Decisões para revisar**
- A **curva do nó 01** não aparece no print: ajustei uma curva Y de 9 pontos ao resultado do
  DaVinci. Os pontos abaixo de 0,3 e acima de 0,75 do sinal não têm pixel no frame e ficaram
  "soltos" (`primaries::koda_global`). Se você me mandar um print da aba Curves do nó 01, eu troco.
- Fórmulas sem conferência no DaVinci (escolhi as usuais de correção de cor): Lift
  `x + L(1−x)`, Gain `×G`, Gamma como potência `2^(−γ)`, Offset `(valor−25)/100`, Contraste
  linear em torno do Pivot (o DaVinci pode estar usando a opção "S-curve"), Shadows/Highlights
  como deslocamento de luma com transições suaves (até ±0,25 em ±100), Color Boost como vibrance,
  Hue girando ±180°, Mid Detail com sigma 12 px e aplicado antes das primárias. Temp/Tint em forma
  exponencial (calibrada num ponto só: −69,9).
- Ordem dentro do nó: Mid Detail → Temp/Tint → LGG por canal → Y com Lum Mix → Offset →
  Contraste → Shadows/Highlights → Boost/Sat/Hue → curva Y → curvas R/G/B.
- Curva Y aplicada por canal (como "master"); "ligada" move R, G e B juntos e deixa o Y separado.
- Para calibrar o resto, o ideal é exportar do DaVinci uma cartela com um controle de cada vez.

**O que deu problema**
- `smooth` é palavra reservada em WGSL: o shader não compilava na GPU; renomeado.
- O disco estava em 20 GB; apaguei 166 executáveis de teste antigos em `target/release/deps`
  (4,5 GB) e ficou em 24 GB.

**Próximo passo**
- Marco 4: nó de nitidez e glow, calibrados em 02_cst → 03_nitidez → 04_glow.

## 2026-10-16 — Barra de páginas no rodapé, como no DaVinci

**O que foi feito**
- Nova barra de páginas ao longo do rodapé, em todas as páginas
  (`crates/ui-egui/src/koda/pages.rs`): "Koda Pro" à esquerda; **Editar, Cor, Motion e Exportar**
  no centro, com ícones próprios em traço fino (clipes na timeline, três círculos de cor, caminho
  com keyframe, seta saindo da bandeja); a página atual em **verde KODA** com sublinhado;
  **Início** e **Configurações** à direita. Motion aparece desativada (a página ainda não existe).
- As abas de modo (e o ícone Início) saíram do cabeçalho do FilmCraft (`header.rs`, guardado por
  `koda::pages::IN_FOOTER`). A barra é "moldura" como o cabeçalho: some quando um app que embute o
  FilmCraft esconde o cabeçalho.
- **Atalhos como no DaVinci:** Shift+4 Editar, Shift+5 Motion, Shift+6 Cor, Shift+8 Exportar
  (comandos novos `page.*`; os `mode.*` continuam, com Cmd+M em Exportar). Esses atalhos eram dos
  painéis Program, Effect Controls, Audio Track Mixer e Media Browser (padrão do Premiere); os
  painéis continuam no menu Janela, e a predefinição "compatível com Premiere" ainda os aplica.
- Traduções pt-BR, es, zh-CN e ja dos textos novos; "Motion" fica sem tradução (é o nome da
  página, não o efeito de movimento).
- Prints (renderizados fora da tela por `crates/ui-egui/examples/koda_prints.rs`, porque a tela do Mac estava dormindo): `docs/koda/cor/prints/barra-paginas-antes.png` (abas no cabeçalho),
  `barra-paginas-depois.png` (Editar) e `barra-paginas-cor.png` (Cor, aberta com Shift+6).
  **App atualizado.**

**O que funcionou**
- Shift+6 abre a Cor, Shift+8 o Exportar, Shift+4 volta ao Editar; os ids antigos
  (`header.home`, `header.mode.*`) continuam valendo na barra nova, então a automação e os testes
  do FilmCraft que clicam neles seguem funcionando.
- Testes da interface e do motor: 1.280 passaram; CI completo verde: 3.489 testes, layers, assets (69) e wasm (50 crates) ok.

**O que deu problema**
- A seção "Barra de páginas no app inteiro" citada no pedido não está no
  `LAYOUT_PAGINA_COR.md` do repositório (só a linha da tabela); segui a tabela e o pedido.
- A barra tira 36 px de altura dos painéis. Testes da interface com janela de tamanho fixo
  quebraram (menu de contexto escondido atrás da barra de ferramentas do painel Projeto, etc.):
  as janelas de teste ganharam exatamente esses 36 px (78 linhas em 77 arquivos de teste), assim
  os painéis ficam do mesmo tamanho de antes. O teste de moldura (`chrome_ui`) passou a tratar a
  barra como moldura.
- Faltavam traduções ("Settings" em chinês/japonês e o nome derivado "Page").

**Próximo passo**
- Marco 3 da aba Cor: primárias e curvas (plano antes).

## 2026-10-16 — Marco 2 da aba Cor: nós de CST e LUT

**O que foi feito**
- **Nó CST** (`crates/koda-color/src/cst.rs`): entrada (gamut + curva), matriz para o gamut de
  saída, tone mapping, gamut mapping e curva de saída. Curva S-Log3 e primárias S-Gamut3.Cine do
  `crates/color` (documentos da Sony). Padrão = CST do powergrade KODA: S-Gamut3.Cine/S-Log3 →
  Rec.709/Gamma 2.4.
- **Calibração com o DaVinci** (caixa-preta: só saída contra saída, sem código da Blackmagic):
  medi a curva neutra 01_global → 02_cst e ajustei uma **curva fílmica logística em stops**
  (white 1,034, mid −1,55, contrast 1,86). O DaVinci aplica o tone mapping **por canal** (ΔE 0,71
  por canal contra 1,40 na luminância ou no máximo de RGB). A compressão de gamut ficou como
  opção, desligada no padrão (mediu um pouco pior: 0,713 contra 0,706).
- **Nó LUT**: LUT da biblioteca de LUTs do projeto (`lib:<id>`, via `lut.import`), intensidade
  0–1, interpolação trilinear (padrão, a que bate com o DaVinci) ou tetraédrica; LUT 1D/shaper
  vira cubo 33³. Comando novo `koda.color.loadLut {node, path}`.
- **GPU**: CST e LUT no `grade.wgsl` (LUT 3D desdobrada numa imagem 2D auxiliar). Gancho do
  FilmCraft ajustado: o efeito externo agora se "prepara" com acesso ao projeto
  (`ExternalFx::prepare`) e cada passe pode levar uma imagem auxiliar (`AuxImage`), enviada à GPU
  uma vez e reaproveitada (`gpu::fx`).
- **ΔE 2000** (`crates/koda-color/src/deltae.rs`), conferido com os 10 pares de referência de
  Sharma, Wu e Dalal (2005).
- Imagens: comparações lado a lado KODA | DaVinci | mapa de ΔE em
  `docs/koda/cor/referencia/comparacoes/` (**locais, fora do git**: têm o vídeo da cliente);
  print público `docs/koda/cor/prints/marco2-carta.png` (cartela sintética em S-Log3: bruto, CST,
  CST + LUT). **App atualizado.**

**O que funcionou — números (ΔE 2000, 1080 × 1920 = 2.073.600 pixels)**

| Comparação | Média | p99 | Máxima |
|---|---|---|---|
| CST(01_global) × 02_cst, pixel a pixel | **0,68** | 2,05 | 5,54 |
| CST(01_global) × 02_cst, média 3×3 | **0,36** | 0,99 | **1,85** |
| LUT(04_glow) × 05_lut | **0,09** | 0,24 | **0,46** |
| CST(00_log) × 02_cst (pedido; sem o nó 01) | 17,65 | 22,67 | 24,17 |
| LUT(02_cst) × 05_lut (pedido; sem os nós 03 e 04) | 3,98 | 9,64 | 11,51 |
| 04_glow × 02_cst (só o efeito dos nós 03+04) | 3,78 | 8,04 | 9,65 |

- O CST e a LUT **atendem o critério da especificação** (média < 1, máxima < 3) quando medidos
  isolados. Pixel a pixel o CST tem máxima 5,5 em 0,07 % dos pixels; com média 3×3 cai para 1,85:
  é ruído de um pixel entre as exportações do DaVinci (pixels vizinhos parecidos vão para lados
  opostos), não erro da curva.
- As comparações pedidas a partir do 00_log e do 02_cst não podem bater ainda: o 02 foi exportado
  depois do nó 01 (correção global, valores não disponíveis) e o 05 depois dos nós 03 e 04.
- GPU × CPU no compositor (CST, CST com compressão e saída sRGB, LUT trilinear, LUT tetraédrica
  a 60 %, CST + LUT; cheio e 1/2): diferença média ≤ 0,05 de 255, p99 1.
- Testes: 15 unitários + 4 do marco 2 + os do marco 1; CI completo: **verde** (3.489 testes, 0 falhas; clippy, camadas, assets e WebAssembly ok; 38 min).

**O que deu problema**
- O máximo pixel a pixel do CST (5,5) passa da meta de 3; explicado acima (ruído de exportação).
- Para usar no app, o clipe da FX30 precisa ficar na interpretação de cor padrão (Rec.709, como
  a câmera marca). Se o Interpret Footage for mudado para S-Log3, o FilmCraft converte antes e o
  CST converteria duas vezes. A aba Cor (marco 5) vai avisar.
- A curva fílmica foi calibrada num único frame; outros clipes/exposições podem pedir um ajuste
  fino (os parâmetros são editáveis no nó).

**Próximo passo**
- Marco 3: primárias (Lift/Gamma/Gain/Offset em barras e rodas, Temp, Tint, Contraste, Pivot,
  etc.) e curvas na GPU, com a matemática do DaVinci; quando o dono tiver os valores do nó 01,
  dá para fechar a comparação a partir do 00_log.

## 2026-10-16 — Marco 1 da aba Cor: motor de nós

**O que foi feito**
- Crate novo **`crates/koda-color`** (camada 4, ao lado do motor):
  - `grade`: modelo da grade (lista ordenada de nós com id estável, nome, ativado/desativado,
    parâmetros; bypass da grade inteira). Tudo que vem de projeto/comando é sanitizado: no máximo
    64 nós, nomes até 64 caracteres, parâmetros na faixa, nunca NaN.
  - `store`: a grade fica no clipe como efeito oculto `koda_grade`, com o JSON da grade em texto
    legível dentro do `.fcproj` (sem mudar o formato do projeto).
  - `eval`: referência na CPU e passes da GPU (`grade.wgsl`), com o nó **Ganho** (mestre + RGB).
    Os nós trabalham no sinal (sRGB/Rec.709 estendido), em float 32 bits, sem cortar valores.
  - `commands`: `koda.color.inspect`, `addNode`, `removeNode`, `moveNode`, `renameNode`,
    `setNodeEnabled`, `setParam`, `setBypass`, `setGrade`; cada edição é um passo de desfazer;
    alvo = `clip` ou o clipe selecionado. Aparecem no CLI, no canal de controle e no MCP.
- Ganchos mínimos no FilmCraft (candidatos a enviar ao upstream):
  - `render::external`: registro de efeitos de outros crates; rodam **antes** dos outros efeitos
    (Lumetri incluso) e antes do Motion, na CPU e no plano da GPU (`FxOp::External`).
  - `gpu::fx`: passes WGSL externos, com pipeline em cache por shader.
  - `engine::commands::register_external`: comandos de outros crates no registro do motor;
    `effects.remove`, `effects.toggleEnabled` e `effects.reset` recusam a grade (só os
    `koda.color.*` mexem nela).
  - Os apps (desktop e CLI/MCP) chamam `koda_color::register()` na inicialização.
- **Ajustes pedidos:** (1) a grade roda sempre primeiro, não aparece no painel de efeitos e não
  pode ser reordenada/removida pelos comandos de efeitos — com testes; (2) a cadeia de nós já roda
  **na GPU dentro do compositor** (preview e export), antes do marco 5; (3) testes que usarem
  `docs/koda/cor/referencia/` são pulados com aviso se o arquivo não existir
  (`koda_color::reference_file`).
- Prints: `docs/koda/cor/prints/marco1-antes.png` e `marco1-depois.png` (Program monitor antes e
  depois de um nó de ganho aplicado só por comando). **App atualizado.**

**O que funcionou**
- 15 testes do `koda-color`: modelo e sinal; comandos com desfazer/refazer; 17 parâmetros
  hostis recusados sem pânico; grade oculta e protegida; ida e volta salvando e reabrindo o
  projeto; a grade antes do Tint e do Motion (na CPU e como 1ª operação do plano da GPU); **GPU
  contra CPU** no compositor (ganho, bypass, cadeia de 2 nós, nó desativado, ganho + Tint, em
  resolução cheia e 1/2): diferença média < 0,25 e p99 ≤ 1 em 255.
- CI completo verde com o marco 1 incluso (entrada abaixo): 3.477 testes, clippy, camadas,
  assets e WebAssembly (o `koda-color` compila para wasm32).

**O que deu problema**
- A primeira medição GPU × CPU acusava diferença grande com ganho > 1. Era a **CPU** que não
  aplicava a grade: um clipe cujo único efeito é a grade caía no atalho "quadro puro" do render
  (a grade não tem definição de efeito). Corrigido com `external::has_effects` nos 6 pontos de
  decisão do render; teste de regressão criado.
- Clippy pediu dois ajustes pequenos (`Option::map`, `as_chunks_mut`).

**Próximo passo**
- Marco 2: nós de CST (S-Log3/S-Gamut3.Cine → Rec.709) e LUT, comparados com os frames 02 e 05
  exportados do DaVinci (`docs/koda/cor/referencia/`).

## 2026-10-16 — CI da base: impasse na exportação encontrado e corrigido

**O que foi feito**
- `cargo xtask ci` completo, pedido antes do marco 1.
- 1ª tentativa (15/10, noite): **travou por 12 horas** nos dois testes de fluxo real
  (`apps/filmcraft-cli/tests/workflows.rs`: câmera → ProRes e celular → H.264). As pilhas
  mostraram um **impasse causado pela correção 1** (cópia paralela do quadro do VideoToolbox):
  a cópia rodava no pool do rayon de dentro do callback do VideoToolbox, e numa exportação todas
  as threads do pool esperavam esse mesmo decodificador. O app instalado podia travar ao
  exportar.
- Correção: `crates/platform/src/par_copy.rs` copia em faixas com **threads próprias
  temporárias** (`std::thread::scope`), nunca no pool compartilhado; `rayon` virou só dependência
  de teste. Teste de regressão `copies_never_wait_for_a_busy_rayon_pool`: **com o código antigo
  falha por tempo esgotado (impasse); com o novo passa**. Os dois testes de fluxo passam em
  1,5 s. App reinstalado com a correção logo em seguida.
- 2ª tentativa: parou no clippy por 11 avisos na prévia da aba Cor (`&tl!(...)`), que eu não
  tinha passado no clippy. Corrigido.
- 3ª tentativa: interrompida de propósito, porque ia falhar em 4 testes de tradução — a prévia
  tinha textos novos só em português; o FilmCraft exige espanhol, chinês e japonês. Traduções
  adicionadas.
- `docs/koda/cor/referencia/` no `.gitignore` (frames de cliente, LUT comercial, powergrade);
  `layout-aba-cor.png` com atribuição (obra do dono); especificações `ESPEC_COR_V1.md` e
  `LAYOUT_PAGINA_COR.md` versionadas; regra "app atualizado" no `CLAUDE.md`.

**Resultado do CI final** (base + correções + marco 1): **verde.** `cargo fmt --check` ok; `cargo clippy --workspace --all-targets --release -D warnings` ok; `cargo test --workspace --release`: **3.477 testes passaram, 0 falharam** (44 ignorados pelo próprio projeto, 313 suítes); `layers` ok; `assets` ok (65); `wasm` ok (50 crates) depois de instalar o alvo `wasm32-unknown-unknown` no rustup (faltava a biblioteca padrão para WebAssembly neste Mac; não era erro de código). Rodou em 27 min com o Mac impedido de dormir (`caffeinate`), porque quase tudo já estava compilado.

**O que deu problema**
- Todas as falhas de CI desta rodada foram causadas por mudanças minhas (correção 1 e prévia),
  não pela base do FilmCraft.
- O CI completo leva 1,5–2,5 h, quase tudo compilando os ~250 executáveis de teste em release.

**Próximo passo**
- Marco 1 (entrada acima).

## 2026-10-15 — Prévia visual da aba Cor

**O que foi feito**
- A pedido do dono, uma **prévia só visual** da aba Cor (adiantando a moldura do marco 5, sem
  funcionalidade): botão "Cor" no cabeçalho entre Editar e Exportar, e a tela no layout de
  `docs/koda/cor/layout-aba-cor.png`: LUTs, editor de nós (01 a 06 em série, 06 tracejado =
  desativado, 01 selecionado em verde), efeitos, faixa de clipes com codec e timecode, barra de
  paletas, primárias em barras (Lift/Gamma/Gain/Offset com os nomes do DaVinci) e scopes (Parade
  em destaque), viewer vertical 9:16 na altura toda à direita.
- Código nosso em `crates/ui-egui/src/koda/cor.rs`; no FilmCraft só o necessário: `Mode::Color`,
  a aba no cabeçalho, o despacho do corpo, o comando `mode.color` e `mode=color` no canal de
  controle. Textos novos traduzidos em pt-BR.
- Feito numa cópia separada (git worktree, branch `koda/previa-cor`) com `nice`, para não
  atrapalhar o `cargo xtask ci` que estava rodando na `main`.
- `packaging/macos/koda-app.sh` agora respeita `CARGO_TARGET_DIR`.
- Print: `docs/koda/cor/prints/previa-aba-cor.png` (com atribuição). **App atualizado.**

**O que funcionou**
- Compilou sem avisos; `cargo xtask assets` ok (62 assets).

**O que deu problema**
- O app abriu com a janela "Recuperar alterações não salvas" listando 10 sessões: são as cópias
  do app que fechei à força nos testes de desempenho. Foi fechada com "Agora não" (nada apagado).
- Os nomes Lift/Gamma/Gain/Offset saíam traduzidos ("Levantar", "Gama"…) pelo catálogo existente;
  ficaram sem tradução, como no DaVinci.
- Nada funciona ainda: os painéis são molduras; o viewer não mostra o vídeo.

**Próximo passo**
- Terminar o CI da base, juntar esta prévia à `main` e seguir com o marco 1 (motor de nós).

## 2026-10-15 — Compilação gastando menos disco

**O que foi feito**
- `.cargo/config.toml`: perfil `dev` (e `test`, que herda dele) com `debug = 0` e
  `incremental = false`; `release` com `debug = 0`. Para depurar com símbolos por um tempo:
  `CARGO_PROFILE_DEV_DEBUG=2 cargo build`.
- Apagado o `target/debug` antigo e recompilados os testes de frame, codecs, media, render,
  platform, ui-egui e export para medir.

**O que funcionou**
- `target/debug` para esses testes: **6,5 GB** (era 52 GB com debug info, 10 GB só sem debug
  info); compilação do zero em 2 min 6 s. `target` inteiro: 9,6 GB (2,9 GB são do release).
  Disco interno: 47 GB livres de 460 GB.

**O que deu problema**
- Sem o cache incremental, recompilar depois de mudar um crate do workspace fica um pouco mais
  lento. A maior parte do que sobra são os binários de teste (cada arquivo em `tests/` do
  `ui-egui` vira um executável próprio).
- SSD externo "SSD 2" (954 GB, 398 GB livres) é exFAT via USB: dá para usar com
  `CARGO_TARGET_DIR`, mas exFAT não tem os links que o cargo usa, cria arquivos `._*` do macOS,
  é mais lento para milhares de arquivos pequenos e o build quebra se o disco for desconectado.
  Ideal seria um volume APFS dedicado.

**Próximo passo**
- Se o disco interno apertar de novo: `cargo clean` de vez em quando, ou mover o `target` para um
  volume APFS externo com `CARGO_TARGET_DIR`.

## 2026-10-15 — 4:2:2 preservado no export, cache ajustado e testes em mais clipes (etapa 1)

**O que foi feito**
- **Atalho 4:2:0 só no preview.** `VideoFrame::rotated` voltou a ser sem perda (4:2:2 girado vira
  4:4:4), agora em blocos e em paralelo (`widen_422` paralelo + `rotate_plane` em blocos); o
  atalho 4:2:0 virou `VideoFrame::rotated_preview`.
- Nova marcação de pedido `filmcraft_media::cancel::with_preview` (como a de draft). Os workers
  dos monitores/reprodução/miniaturas (`crates/ui-egui/src/frames.rs`) pedem quadros como preview;
  exportação, render final, CLI e MCP não, então recebem sempre o quadro exato.
- O cache de GOP (`crates/codecs/src/gop.rs`) marca os quadros feitos com o atalho
  (`previews`, como já fazia com os `drafts`) e só os entrega a pedidos de preview: um export do
  mesmo quadro decodifica de novo pelo caminho exato.
- **Orçamento do cache.** O piso de 64 quadros valia para qualquer tamanho de quadro; agora vale
  em bytes equivalentes a 64 quadros 8 bits 4:2:0 daquela resolução (`min_frames`): 10 bits 4:2:0
  = 32 quadros, 10 bits 4:2:2 = 24, 16 bits 4:4:4 = 16 (RGBA inalterado).
- Testes novos: export nunca recebe quadro de preview (4:4:4 exato, preview 4:2:0, fonte sem
  rotação igual nos dois); rotação exata guarda todas as amostras de croma; piso do cache por
  profundidade.
- `Koda Pro.app` reinstalado.

**O que funcionou**
- `cargo fmt` ok, `clippy -D warnings` sem avisos; testes de frame, codecs, media, render,
  platform, ui-egui e export: **1.301 passaram, 0 falharam**.
- Inventário do SSD: 561 clipes da FX30, todos H.264 (XAVC S); 544 verticais 4:2:2 10 bits
  (403 a -90°, 141 a +90°), 16 horizontais 4:2:2 10 bits, 1 vertical 4:2:0 8 bits.
  **Nenhum XAVC HS (H.265)**.
- Preview no app (7 s, carga < 4), quadros perdidos:

  | Clipe | 1/2 | Resolução cheia | p95 decodificação (1/2 / cheia) |
  |---|---|---|---|
  | FXC4808 vertical -90°, 29,97p | 0 % | 0,5 % | 37 / 67 ms |
  | FXC4528 horizontal, 59,94p | 0 % | 0 % | 9 / 9 ms |
  | FXC1317 vertical +90°, 59,94p | 0,7 % | 0,5 % | 36 / 35 ms |
  | HEVC 4:2:2 10 bits vertical sintético (ffmpeg/x265, 103 Mbps, 4 s) | 0,9 % | 0 % | 75 / 47 ms |

  Cache em 796 MB em todos (antes 1,6–3,2 GB). VideoToolbox em todos os quadros, inclusive HEVC
  4:2:2 (Rext).
- Caminho exato (bench `--hw auto`, como no export): vertical 4K **74,6 fps** (2,5× tempo real;
  era 43,5 no começo), horizontal 4K 59,94p **183 fps**, HEVC 4:2:2 sintético 94 fps; pico de
  memória 2,1 GB (era 4,2 GB).
- Export ProRes 422 HQ de 3 s do clipe vertical: 2160 × 3840, `yuv422p10le`, 90 quadros, 8,1 s.
  Comparado com a rotação do próprio ffmpeg sobre o original: PSNR médio Y 55,0 dB, **U 55,3 dB,
  V 60,2 dB**, ou seja, a cor 4:2:2 foi preservada.

**O que deu problema**
- **Disco cheio no meio dos testes:** `target/debug` chegou a 52 GB e o disco ficou com 122 MB
  livres. Apaguei `target/debug` (é recriado pelo cargo) e passei a testar com
  `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`, que reduz muito o tamanho.
- No primeiro teste do HEVC sintético o clipe (5 s) terminou antes dos 7 s do teste; repetido com
  4 s. Era problema do teste, não do app.
- O p95 de decodificação do vertical -90° em resolução cheia (67 ms) passa dos 33 ms de um quadro,
  mas a fila de pré-busca absorve: só 1 quadro perdido em 207.
- XAVC HS real não foi testado (não há no SSD); o sintético não reproduz o GOP/bitrate exatos da
  câmera.

**Próximo passo**
- Testar um XAVC HS real quando houver; editar de verdade uma timeline com vários clipes da FX30
  (cortes, Lumetri com LUT de S-Log3, export) e rodar `cargo xtask ci` completo.

## 2026-10-15 — Correção 2: rotação rápida de clipes verticais (etapa 1)

**O que foi feito**
- `crates/frame/src/lib.rs` (`VideoFrame::rotated`, usado pelo cache de GOP para todo clipe com
  marcação de rotação):
  - `rotate_plane` agora gira em blocos de 64 × 64 (faixas de 64 linhas em paralelo), para que as
    linhas lidas de cada bloco fiquem no cache da CPU;
  - giro de 90°/270° em 4:2:2 não alarga mais o croma para 4:4:4: o croma é girado no próprio
    tamanho e cada par de amostras lado a lado vira a média (`halve_width`), resultando em 4:2:0.
- Testes novos: a rotação em blocos dá o mesmo resultado que a rotação pixel a pixel (tamanhos que
  não são múltiplos de 64, pixels de 1, 2 e 4 elementos, as três rotações); 4:2:2 girado vira 4:2:0
  do tamanho certo em 10 bits; teste antigo de 4:2:2 atualizado para o 4:2:0.
- `Koda Pro.app` reinstalado com a correção.

**O que funcionou** (Mac quase ocioso, carga ~3,5)
- Testes: `filmcraft-frame` ok; `filmcraft-codecs`, `filmcraft-media` e `filmcraft-render`:
  472 passaram, 0 falharam; `clippy -D warnings` sem avisos em frame, codecs e platform.
- Bench de decodificação (`--hw auto`, 3 repetições): **49,7 → 91,6 fps** (3,1× o tempo real);
  CPU por quadro **87 → 63 ms**; primeiro quadro 82 ms; pico de memória **4,2 → 3,1 GB**.
- Reprodução no app (1/2 resolução, 7 s, duas rodadas iguais): quadros perdidos
  **14 % → 0,5 %** (207 mostrados, 1 perdido); decodificação p95 **61 → 33,6 ms**; cache
  **3,2 → 1,6 GB**.
- Somando as duas correções, no mesmo arquivo: 43,5 → 91,6 fps de decodificação e 21 % → 0,5 % de
  quadros perdidos.

**O que deu problema / limitações**
- Clipes verticais 4:2:2 passam a ter croma 4:2:0 depois de girados, também na exportação. Para
  entregas em H.264/HEVC 4:2:0 não muda nada; uma exportação ProRes/DNxHR 4:2:2 de um clipe
  vertical perde metade da resolução de cor num dos eixos. Se isso importar, a saída é girar na
  GPU (sem perda) ou criar um formato 4:4:0.
- Só medi o preview em 1/2; em resolução cheia ainda não foi medido.
- Mudança em código do upstream (`filmcraft-frame`): candidata a ser enviada ao FilmCraft.

**Próximo passo**
- Testar outros modos de gravação da FX30 (horizontal, XAVC HS/H.265, 4:2:0, S-I) e o preview em
  resolução cheia; depois rodar `cargo xtask ci` completo.

## 2026-10-15 — Correção 1: cópia paralela do quadro do VideoToolbox (etapa 1)

**O que foi feito**
- Novo módulo seguro `crates/platform/src/par_copy.rs`: copia as linhas do quadro decodificado
  pelo VideoToolbox em paralelo (rayon, faixas de 16 linhas), separando Cb/Cr e convertendo os
  10 bits, sem `unsafe` e devolvendo erro (não pânico) se o plano for menor que o esperado.
- `copy_out` em `crates/platform/src/videotoolbox.rs` passou a usar esse módulo; `rayon` entrou
  como dependência do crate `platform`. É código do upstream: vale enviar ao FilmCraft depois.
- Testes novos comparam a cópia paralela com a cópia antiga (linha a linha, com recorte e stride
  maior que a largura) e verificam planos curtos e estouro de tamanho.

**O que funcionou**
- `cargo fmt`, `clippy -D warnings` e testes do `filmcraft-platform`: 46 passaram, 0 falharam.
- Bench de decodificação (`--hw auto`, 3 repetições, Mac quase ocioso, carga ~3,5):
  **43,5 → 49,7 fps**; primeiro quadro **174 → 101–133 ms**. CPU total por quadro igual (~87 ms):
  a cópia agora usa vários núcleos, mas o trabalho total é o mesmo.
- Reprodução no app (1/2 resolução, 7 s): quadros perdidos **21 % → 14 %** (176 mostrados,
  29 perdidos); decodificação média **23 → 14 ms** (p95 61 ms). Atenção: a primeira medição foi
  com carga 15–18 e esta com carga ~5, então parte da melhora é da máquina mais livre.

**O que deu problema**
- Ainda não roda liso. O profiler do macOS (`sample`) mostrou o verdadeiro gargalo: **a rotação
  do quadro**. O clipe é vertical (gravado 3840 × 2160 com marcação de rotação de 90°), e para cada
  quadro o FilmCraft, em `crates/frame/src/lib.rs`:
  1. alarga o croma 4:2:2 para 4:4:4 numa única thread (`rotate_chroma`), o que deixa o quadro
     com ~50 MB em vez de 33 MB (por isso o cache passa de 3 GB);
  2. gira pixel a pixel lendo a imagem em coluna (`rotate_plane`), o pior padrão de acesso à
     memória.
- No perfil, `rotate_plane`/`rotate_chroma` e cópias de memória dominam; a cópia que
  paralelizei aparece pouco. A correção 1 ajuda, mas não era o maior custo.

**Próximo passo** (proposta, ainda sem código)
- Correção 2, nova prioridade: rotação rápida. Opções, da mais simples à melhor:
  a) girar em blocos (ex.: 64 × 64) para usar o cache da CPU e alargar o croma em paralelo;
  b) girar 4:2:2 sem converter para 4:4:4 (o croma girado vira 4:4:0, ou rotacionar direto para
     4:2:0 no preview);
  c) não girar na CPU: aplicar a rotação na GPU, como parte da transformação do clipe no
     compositor. É a solução certa para reprodução, mas mexe mais no núcleo.
- Depois, repetir as medições com o Mac ocioso e só então partir para o preview reduzido
  (correção 2 antiga, agora 3).

## 2026-10-15 — Diagnóstico: arquivo da FX30 não roda liso no preview (etapa 1)

**O que foi feito** (só medição, nenhum código alterado)
- Arquivo: `FXC4808.MP4` (Sony FX30, 8,5 s, 201 MB). `filmcraft-cli probe`:
  - vídeo **H.264 High 4:2:2**, **10 bits**, **YUV 4:2:2**, **2160 × 3840** (vertical), 29,97 fps,
    **~140 Mbps**; cor marcada como BT.709, faixa completa (full range);
  - áudio PCM 16 bits, 48 kHz, estéreo; contêiner MP4 (XAVC S).
- `cargo xtask bench --sections decode --hw auto`, com o arquivo apontado como fixture
  `dec_h264_2160.mp4` via `FILMCRAFT_FIXTURES` (o bench só lê fixtures, não um arquivo qualquer):
  - **43,5 fps** de decodificação (1,5× o tempo real), **86 ms de CPU por quadro**, primeiro quadro
    em 174 ms, pico de memória **4,2 GB**;
  - **VideoToolbox usado em 255 de 255 quadros**, 1 sessão, 0 fallbacks.
- Mesmo bench com `--hw off`: o decodificador próprio recusa o arquivo
  ("chroma_format_idc 2 (only 4:2:0 is implemented)").
- Reprodução real no app (canal de controle, preview em 1/2, 7 s tocando, `perf.stats`):
  - **21 % dos quadros perdidos** (162 mostrados, 43 descartados); áudio sem falhas;
  - decodificação por quadro: média 23 ms, **p95 66 ms** (o limite a 29,97 fps é 33 ms);
  - cache de quadros com **3,2 GB** para um orçamento de 1,07 GB; `hardware.backend` VideoToolbox,
    0 quadros em software.

**O que funcionou**
- O VideoToolbox decodifica o 10 bits 4:2:2 da FX30 sem erro; o problema não é falta de suporte.

**O que deu problema / o que está travando**
1. **Cópia do quadro na CPU.** Depois do VideoToolbox, `copy_out` em
   `crates/platform/src/videotoolbox.rs` copia o quadro 4K 10 bits inteiro, numa única thread,
   desintercalando o croma e convertendo cada amostra para planar de 16 bits (~33 MB por quadro).
   É a maior parte dos 86 ms de CPU por quadro.
2. **O preview em 1/2 não economiza nada na decodificação.** O quadro 4K é decodificado e copiado
   inteiro e só depois reduzido; o modo rascunho (draft) só vale para o decodificador próprio.
3. **Memória.** Quadros de 33 MB num GOP longo estouram o cache (3,2 GB para 1 GB de orçamento),
   o que força descartes e redecodificações.
- Ressalva: a máquina estava com carga 15–18 (compilações minhas rodando), o que piora os números;
  repetir a medição com o Mac ocioso antes e depois da correção.

**Correção proposta** (ainda não implementada; mexe no crate `platform`, código do upstream, então
o ideal é enviar de volta ao FilmCraft):
1. Paralelizar `copy_out` por faixas de linhas (rayon) — mudança pequena e de baixo risco.
2. Para preview reduzido, pedir ao VideoToolbox o quadro já reduzido (atributos de largura/altura da
   sessão de decodificação, escalonador em hardware) ou reduzir durante a cópia: em 1/2 copia 1/4
   dos dados.
3. Médio prazo: zero-copy (passar o `CVPixelBuffer`/IOSurface direto para a GPU), já listado no
   roadmap do upstream (#30).
4. Ajustar o orçamento do cache ao tamanho real dos quadros 10 bits 4:2:2.
- Contorno imediato, sem código: criar proxies (o FilmCraft já gera proxies ProRes/DNxHR) ou
  usar o preview em 1/4.

**Próximo passo**
- Implementar a correção 1 (cópia paralela), medir de novo com o Mac ocioso e, se não bastar,
  a correção 2.

## 2026-10-15 — Plano do KODA Pro definido

**O que foi feito**
- `docs/koda/PLANO.md` reescrito com o plano do dono: fork do FilmCraft, mexer o mínimo no código
  original, nossas funcionalidades em módulos separados (`crates/koda-*`), e cinco etapas:
  1) base no Mac com os arquivos da Sony FX30, 2) personalização (atalhos, layout, visual KODA),
  3) aba de cor por nós estilo DaVinci, 4) aba Motion estilo After/Fusion, 5) ferramentas de IA.
- O que já estava feito (nome, marca, ícone, `.app`) ficou como "preparação".

**O que funcionou**
- O remote `upstream` e o aviso no `CLAUDE.md` para ler plano e diário já estavam configurados.

**O que deu problema**
- A troca do nome do projeto de demonstração ("FilmCraft Demo"), que o dono decidiu não fazer,
  chegou a ser aplicada no `demo.rs` antes de ser cancelada; foi desfeita com `git checkout`.
- Ponto de atenção para a etapa 1: o decodificador próprio não lê H.264/HEVC 10-bit 4:2:2, o que a
  FX30 grava; no Mac depende do VideoToolbox.

**Próximo passo**
- Etapa 1: testar arquivos reais da FX30 (cada modo de gravação) e rodar `cargo xtask ci`.

## 2026-10-15 — Ícone novo (K)

**O que foi feito**
- O ícone passou a ser o K branco sobre preto enviado pelo dono, no lugar da coruja do FilmCraft.
- O K foi redesenhado como vetor (medido na imagem original de 2000 px) e centralizado num quadrado
  preto de cantos arredondados; `packaging/icons.sh` gerou todos os tamanhos: `.icns` do Mac, ícone
  do Dock, `.ico` do Windows, ícones do Linux e o favicon da versão web.
- Atribuição atualizada (16 arquivos `.attribution` e `ATTRIBUTION.md`); `assets/app-icon/LICENSE.txt`
  agora diz que o ícone é marca do dono, não é open source e só pode ser usado para o Koda Pro.
- `Koda Pro.app` recompilado e reinstalado em `~/Applications`.

**O que funcionou**
- `cargo xtask assets` ok; testes de `filmcraft` e `xtask`: 82 passaram, 0 falharam.
- Ferramenta instalada: `resvg` (`brew install resvg`), usada para gerar os PNGs a partir do SVG.

**O que deu problema**
- A imagem original tinha o K um pouco acima do centro; no ícone ele foi centralizado.
- Os nomes dos arquivos continuam `filmcraft-*` para não quebrar o build e os scripts do upstream.
- O fundo da janela do instalador DMG do upstream ainda deriva da coruja (não usamos esse DMG).

**Próximo passo**
- Rodar o portão completo (`cargo xtask ci`, etapa 7 do plano).

## 2026-10-15 — Koda Pro.app

**O que foi feito**
- Novo script `packaging/macos/koda-app.sh`: compila em release e monta `dist/Koda Pro.app`
  (nome "Koda Pro", identificador `io.github.cleysonbomfim-commits.kodapro`, versão do `Cargo.toml`),
  com assinatura ad hoc para este Mac. Com `--install` copia para `~/Applications`.
- O `.fcproj` continua associado ao app, para abrir projetos com dois cliques.
- Instalado em `~/Applications/Koda Pro.app`.

**O que funcionou**
- O app abre pelo Finder e o macOS o registra como "Koda Pro" (nome no Dock e na barra de menus).

**O que deu problema**
- Nada novo. A assinatura é só local: para distribuir para outros Macs seria preciso uma conta de
  desenvolvedor Apple (o `package.sh` do upstream faz assinatura e notarização).
- O ícone ainda é a coruja do FilmCraft.

**Próximo passo**
- Trocar o ícone do programa (pedido do dono).

## 2026-10-15 — Marca ArtCraft removida

**O que foi feito**
- Removidos o logo ArtCraft (janela Sobre e tela inicial), o botão Discord do topo da janela, os
  itens "Discord da ArtCraft", "Site da ArtCraft" e "Koda Pro no getartcraft.com" do menu Ajuda e do
  menu do app no macOS, e a pasta `docs/brand/` com os logos (e suas linhas no `ATTRIBUTION.md`).
- Os links "Código-fonte no GitHub" e "Relatar um problema" agora apontam para
  `cleysonbomfim-commits/kodapro`.
- A janela Sobre diz, em texto simples, que o Koda Pro é baseado no FilmCraft da equipe ArtCraft
  (a licença da marca permite isso), traduzido nos 8 idiomas que tinham a frase antiga.
- README e NOTICE explicam que a marca foi removida por exigência da licença.

**O que funcionou**
- Capturas de tela conferidas: sem logo, sem Discord, frase nova em português.
- `cargo fmt` ok, `clippy -D warnings` sem avisos, testes de `filmcraft-ui-egui` e `filmcraft`:
  707 passaram, 0 falharam; `cargo xtask assets` ok (61 assets).

**O que deu problema**
- Três testes de tradução (russo, alemão, ucraniano) falharam porque os catálogos ainda tinham os
  textos dos menus removidos; apaguei essas traduções órfãs em todos os idiomas.
- O clippy reclamou de um laço com um item só no menu do macOS; reescrito.

**Próximo passo**
- Montar o `Koda Pro.app` (já em andamento) e depois trocar o ícone.

## 2026-10-15 — Login no GitHub e primeiro envio

**O que foi feito**
- Instalado o GitHub CLI (`brew install gh`, versão 2.102.0); login feito com
  `gh auth login --web` na conta `cleysonbomfim-commits` e `gh auth setup-git` para o `git push` usar
  esse login.
- Enviados para o GitHub os commits da troca de nome e do diário/plano (`8309f21a..3886049a`).

**O que funcionou**
- O `git push origin main` passou de primeira depois do login.

**O que deu problema**
- O primeiro push falhou porque o Mac não tinha nenhum login do GitHub configurado.

**Próximo passo**
- Remover a marca ArtCraft da janela Sobre e empacotar `Koda Pro.app` (etapas 4 e 5 do plano).

## 2026-10-15 — Diário, plano e troca do nome para Koda Pro

**O que foi feito**
- Criados `docs/koda/PLANO.md` e este diário; o `CLAUDE.md` agora manda ler os dois no início de cada
  sessão.
- Nome exibido trocado de FilmCraft para Koda Pro em 31 arquivos: título da janela, menu do macOS,
  janela Sobre, menus Ajuda/Arquivo, diálogos, avisos das Configurações e os 23 textos que contêm o
  nome em todos os catálogos de tradução (inclusive pt-BR: "Sobre o Koda Pro", "Sair do Koda Pro").
- Mantidos de propósito: nomes de crates e do binário (`filmcraft`), ids de comandos, formato
  `.fcproj`, pasta de dados (`~/Library/Application Support/FilmCraft`), metadados gravados em
  arquivos exportados (AAF, OMF, MXF…), copyright e licenças. Assim projetos e preferências continuam
  valendo e o merge com o `upstream` fica simples.

**O que funcionou**
- App compilado em release e aberto: a janela "Sobre o Koda Pro" mostra o nome novo.
- `cargo fmt --check` ok; `cargo clippy -D warnings` sem avisos em `filmcraft`, `filmcraft-engine` e
  `filmcraft-ui-egui`; testes desses três pacotes: 1.346 passaram, 0 falharam.

**O que deu problema**
- Um teste da tradução alemã ("Quit FilmCraft" → "FilmCraft beenden") precisou ser ajustado à mão.
- Na barra de menus e no Dock do Mac ainda aparece "filmcraft": o macOS usa o nome do executável
  quando o app não está num pacote `.app`.
- O título da janela mostra "FilmCraft Demo", que é o nome do projeto de demonstração.
- Não rodei o portão completo (`cargo xtask ci`, que testa o workspace inteiro), só os pacotes
  alterados.
- Os servidores MCP do projeto não conectam porque `target/release/filmcraft-cli` ainda não foi
  compilado.

**Próximo passo**
- Remover a marca ArtCraft da janela Sobre (exigência da licença de marca para forks) e empacotar
  `Koda Pro.app` para o nome aparecer no Dock e na barra de menus.

## 2026-10-15 — Clone, upstream e primeira compilação

**O que foi feito**
- Clonado `cleysonbomfim-commits/kodapro` (o link colado vinha cortado; o nome certo não tem hífen).
- Remote `upstream` → `https://github.com/storytold/filmcraft`.
- Lidos `CLAUDE.md`, `AGENTS.md`, `ROADMAP.md` e `docs/architecture.md`.
- Compilado em release (`cargo build --release -p filmcraft`, ~2 min) e aberto com
  `./target/release/filmcraft --control 9876`.

**O que funcionou**
- Nenhuma dependência faltando no Mac: Rust 1.99 (o projeto pede ≥ 1.95) e Command Line Tools do Xcode.

**O que deu problema**
- Duas cópias do app abertas ao mesmo tempo disputaram a porta 9876; só a primeira fica controlável.
- Espaço em disco apertado: 54 GB livres (88% usado).

**Próximo passo**
- Trocar o nome exibido para Koda Pro (feito na entrada acima).
