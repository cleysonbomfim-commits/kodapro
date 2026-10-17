# Resumo da noite — 16 para 17/10/2026

Trabalho feito sozinho, sem perguntas, na ordem pedida. Detalhes de cada etapa estão no
[DIARIO.md](DIARIO.md).

## O que ficou pronto (um commit e um envio ao GitHub por marco)

| Marco | Commit | O que é |
|---|---|---|
| 3 — primárias e curvas | `b8593a39` | Nó `primaries`: Lift/Gamma/Gain (Y R G B), Offset (R G B), Temp, Tint, Contraste, Pivot, Mid Detail, Color Boost, Shadows, Highlights, Sat, Hue, Lum Mix e curvas Y/R/G/B, com os padrões do DaVinci, na CPU e na GPU |
| 4 — nitidez e glow | `66545a84` | Nós `sharpen` e `glow` (Soft Light, Screen, Add, Normal; ganho; highlight recovery), na CPU e na GPU, calibrados contra os nós 03 e 04 |
| 6 — motor dos powergrades | `cdb296b4` | Powergrades `.kpg` com as LUTs embutidas, aplicar em clipes, copiar a grade de um clipe para outros, copiar/colar nó; tudo por comandos com desfazer |
| 5 — tela da página Cor | `fa597432` | A página Cor com o layout do DaVinci de referência, ligada à grade real (nós, primárias em barras e rodas, curvas, CST, LUT, efeitos, galeria, scopes, viewer) |

O app foi reinstalado depois de cada marco (`~/Applications/Koda Pro.app`).

CI completo no fim (`cargo xtask ci`): **verde** — fmt, clippy, 3.508 testes passando (0 falhas), layers, assets (72) e wasm (50 crates).

## Números de ΔE 2000 (frames do DaVinci, 1080 × 1920)

| Comparação | Média | p99 | Máxima | Meta (média < 1, máx < 3) |
|---|---|---|---|---|
| Primárias do nó 01 (valores do print + curva ajustada) × 01_global | **0,68** | 1,67 | **2,12** | atende |
| Nitidez(02_cst) × 03_nitidez | **0,14** | 0,37 | **0,61** | atende |
| Glow(03_nitidez) × 04_glow | **0,81** | 2,67 | 4,88 | média sim, máxima não |
| **Nitidez + glow(02_cst) × 04_glow** (o pedido) | **0,81** | 2,68 | 4,79 | média sim, máxima não |
| (marco 2) CST(01) × 02_cst | 0,68 | 2,05 | 5,54 (1,85 em média 3×3) | — |
| (marco 2) LUT(04) × 05_lut | 0,09 | 0,24 | 0,46 | atende |

GPU contra CPU, em todos os nós novos (cheio e meia resolução): diferença média ≤ 0,05 de 255.

**Achado da noite:** o print `davinci-color-referencia.png` mostra os valores do nó 01 (Temp
−69,9, Tint −5,78, Shadows 17,5, o resto no padrão). Foi isso que permitiu calibrar Temp/Tint.

## Decisões para você revisar

1. **Curva do nó 01:** o print não mostra a curva; ajustei uma curva Y de 9 pontos ao resultado
   do DaVinci. Um print da aba Curves do nó 01 resolve.
2. **Fórmulas das primárias** que o DaVinci não publica (escolhi as usuais): Lift `x + L(1−x)`,
   Gain `×G`, Gamma `x^(2^−γ)`, Offset `(valor−25)/100`, Contraste linear no Pivot (o DaVinci pode
   usar a opção "S-curve"), Shadows/Highlights como deslocamento de luma, Color Boost como
   vibrance, Hue ±180°, Mid Detail com sigma 12 px antes das primárias. Temp/Tint calibrados num
   ponto só. Uma cartela exportada do DaVinci com um controle de cada vez fecha essas dúvidas.
3. **Nós 03 e 04:** sem os valores reais, os padrões foram ajustados aos frames. O "Soft Light"
   que bate é a imagem sobre o glow (fórmula W3C). Sobra erro em halos (o glow do DaVinci parece
   ter várias escalas de espalhamento); a máxima do glow (4,9) passa da meta.
4. **Powergrades:** extensão `.kpg`, pasta `~/Library/Application Support/FilmCraft/koda/powergrades`;
   colar/aplicar **substitui** a grade (como o DaVinci), com opção de acrescentar; a área de cópia
   vale enquanto o app está aberto.
5. **Página Cor:**
   - o cabeçalho do FilmCraft continua em cima (botões da janela do macOS), então a página fica
     ~38 px mais baixa que no DaVinci;
   - o viewer usa o monitor de programa do FilmCraft;
   - a paleta Primárias/Curvas edita só nós do tipo primárias (no DaVinci todo nó tem tudo);
   - ainda sem atalhos de teclado (colidiriam com os do FilmCraft);
   - estado da página (nó selecionado, paleta) só na memória da janela.

## O que deu problema

- `smooth` é palavra reservada em WGSL: o shader não compilava; renomeado.
- O máximo do glow não chegou abaixo de 3 (ver item 3).
- O disco ficou em 20 GB duas vezes; apaguei executáveis de teste antigos em
  `target/release/deps` (4,5 GB e depois 3,7 GB), sem `cargo clean`.
- Nada ficou travado mais de 1 hora; nenhum item foi pulado.

## O que testar no app de manhã

1. Abra o Koda Pro e um projeto com o clipe da FX30; vá para a página **Cor** (Shift+6).
2. Na **Biblioteca de efeitos**, clique em *Primárias e curvas*, *Transformação de espaço de cor*,
   *Nitidez* e *Glow*: os nós aparecem em escada no editor e a imagem do viewer muda.
3. Selecione o nó de primárias e **arraste** os campos (Temp, Cont…), as **barras** e a roda de
   arrastar; troque para **rodas** (botão ◎). Cmd+Z deve desfazer um arraste inteiro de uma vez.
4. Paleta **Curvas** (ícone da curva na barra de paletas): clique para pôr pontos e arraste.
5. Importe a `Orson.cube` em **LUTs › Importar LUT…** e clique na miniatura: entra um nó LUT.
6. **Galeria** (barra de cima): *+ Salvar grade*, depois aplique em outro clipe da faixa de clipes.
7. Botão **Antes / depois** no topo do viewer.
8. Compare com o DaVinci: `docs/koda/cor/referencia/comparacoes/pagina_cor_lado_a_lado.png` (só
   local) e os mapas de ΔE em `referencia/comparacoes/` (primárias, nitidez, glow).
