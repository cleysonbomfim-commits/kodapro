# KODA Pro — layout da página Cor (referência: DaVinci do dono)

Referência visual: `davinci-color-referencia.png` (print do dono, 1915 × 1049, DaVinci Resolve Studio 21, página Color com o powergrade KODA).

**Regra:** copiar o posicionamento, as proporções, a densidade e o comportamento dos painéis com fidelidade. **Não copiar** ícones, logo, nomes de marca nem elementos gráficos da Blackmagic: cada ícone é redesenhado no mesmo tamanho e função, em traço simples, no visual KODA.

**Método de trabalho:** a cada mudança de layout, tirar um print da página Cor do KODA Pro na mesma resolução da referência, colocar lado a lado com o print do DaVinci (salvar a comparação em `docs/koda/cor/prints/`) e ajustar até as divisões dos painéis coincidirem. Não considerar o layout pronto antes dessa comparação.

## Grade da página (proporções medidas no print)

Medidas em % da largura (1915 px) e da altura (1049 px) da janela. Os painéis devem acompanhar essas proporções ao redimensionar a janela.

| Região | Horizontal | Vertical | Conteúdo |
| --- | --- | --- | --- |
| Barra superior | 0–100% | 0–3% | Botões de painel à esquerda (Galeria, LUTs, Mídia, Clipes), nome do projeto no centro, botões à direita (Export rápido, Timeline, Nós, Efeitos, Lightbox). Cada botão liga/desliga seu painel |
| Barra de ferramentas dos painéis | 0–71% | 3–5,5% | Ferramentas do navegador de LUTs, do editor de nós (seleção, modo "Clip") e do painel de efeitos (abas Biblioteca/Configurações, busca) |
| Navegador de LUTs | 0–7,5% | 5,5–51% | Árvore de pastas de LUTs, uma por linha, com seta para expandir |
| Grade de miniaturas de LUT | 7,5–26,5% | 5,5–51% | Miniaturas das LUTs da pasta selecionada (pré-visualização ao passar o mouse) |
| Editor de nós | 26,5–49,5% | 5,5–51% | Grafo de nós em série, entrada à esquerda e saída à direita, indicador de páginas embaixo |
| Biblioteca de efeitos | 49,5–71% | 5,5–51% | Categorias recolhíveis (Blur, Cor, Glow…), cada item com ícone pequeno à esquerda e nome |
| Viewer | 71,3–100% | 3–97% | Altura total. Barra no topo (zoom %, nome da timeline, timecode), imagem centralizada (vertical ocupa a altura), barra de scrub, transporte embaixo (play, stop, loop, anterior/próximo) e timecode grande à direita |
| Faixa de clipes | 0–71% | 52–63,5% | Cada clipe: número em selo, timecode e trilha na linha de cima, miniatura no meio, codec embaixo (ex.: "H.264 High 4:2:2 L5.1"). Clipe atual com contorno de destaque. Rolagem horizontal |
| Barra de paletas | 0–71% | 64,5–67,5% | Fileira de ícones à esquerda (uma por paleta: Primárias, HDR, Mixer RGB, Curvas, Qualifier, Windows, Tracker, Blur, Key, Sizing…), e à direita os ícones de Keyframes, Scopes e Info. Paletas que ainda não existem aparecem desativadas |
| Paleta Primárias | 0–35,5% | 68–97% | Detalhe abaixo |
| Scopes | 35,5–71% | 68–97% | Cabeçalho "Scopes" com seletor de tipo (Parade padrão) e botões de opções; gráfico em fundo preto com escala 0–1023 marcada a cada 128 |
| Barra de páginas | 0–100% | 97–100% | Nome do app à esquerda; ícones das páginas no centro (Editar, Cor, Motion, Exportar), página atual sublinhada com o acento; Início e Configurações à direita |

**Barra de páginas no app inteiro.** A barra de páginas do rodapé vale para todas as páginas (Editar, Cor, Motion, Exportar), como no DaVinci, e substitui as abas de modo do cabeçalho herdadas do FilmCraft. Cada página tem um ícone próprio (traço fino, visual KODA) com o nome embaixo ao passar o mouse; a página atual fica marcada com o acento verde. Atalhos como no DaVinci: Shift+4 Editar, Shift+6 Cor, Shift+5 Motion, Shift+8 Exportar. Páginas que ainda não existem (Motion) aparecem desativadas.

## Paleta Primárias (modo barras)

- Cabeçalho: "Primárias – Barras" à esquerda; à direita, botões para alternar entre Barras e Rodas e o reset.
- Linha de cima, campos numéricos com rótulo à esquerda: **Temp**, **Tint**, **Cont**, **Pivot**, **MD** (com o ícone de auto-balance à esquerda de tudo).
- Quatro grupos lado a lado: **Lift**, **Gamma**, **Gain**, **Offset**. Cada grupo tem título, botão de reset, um ícone de seleção de amostra (nos três primeiros), barras verticais Y/R/G/B (Offset só R/G/B), campos numéricos embaixo de cada barra com sublinhado na cor do canal e uma roda de arrastar (jog) horizontal embaixo.
- Valores padrão como no DaVinci: Lift/Gamma 0,00; Gain 1,00; Offset 25,00; Cont 1,000; Pivot 0,435.
- Linha de baixo: **Col Boost**, **Shad**, **Hi/Light**, **Sat**, **Hue**, **L. Mix** (padrões 0,00 / 0,00 / 0,00 / 50,00 / 50,00 / 100,00).
- Arrastar no campo numérico muda o valor; duplo clique edita; duplo clique no título reseta o grupo.

## Editor de nós

- Nó: retângulo com rótulo acima (nome editável), miniatura da imagem no centro, número e ícones das ferramentas usadas embaixo (primárias, curvas, LUT, efeito).
- Entradas/saídas: quadradinhos coloridos nas laterais (verde para RGB, azul para alfa), como pontos de conexão.
- Nó selecionado com contorno no acento; nó desativado com número e rótulo apagados.
- Entrada da grade à esquerda e saída à direita, como pontos ligados por linhas finas.

## Estilo visual

- Tema cinza escuro neutro, como no print: barras superior e de páginas em torno de `#17181a`; fundo dos painéis em torno de `#28282e`; viewer `#191919`; faixa de clipes e scopes em preto.
- Texto claro (`#f1f1f1` para títulos, cinza médio para rótulos), em tamanho pequeno (11–12 px), sem negrito exagerado.
- Separadores de 1 px entre painéis; cabeçalhos de painel baixos (~24 px).
- **Acento KODA:** verde no lugar do vermelho/laranja do DaVinci (seleção de nó, clipe atual, página atual, valores alterados). Branco e preto como base.
- Ícones em traço fino, monocromáticos, todos do mesmo tamanho.
