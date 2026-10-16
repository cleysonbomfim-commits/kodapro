# koda-color — aba Cor do KODA Pro

Grades de cor por clipe, feitas de nós em série, no estilo do DaVinci Resolve. Especificação:
[`docs/koda/cor/ESPEC_COR_V1.md`](../../docs/koda/cor/ESPEC_COR_V1.md).

## O que tem aqui

| Módulo | O que faz |
|---|---|
| `grade` | Modelo: `Grade` (lista ordenada de `Node`, bypass, ids estáveis). Tudo que vem de projeto, powergrade ou comando é sanitizado (máx. 64 nós, nomes até 64 caracteres, parâmetros dentro da faixa, nunca NaN). |
| `store` | A grade fica no clipe como efeito oculto `koda_grade`, com o JSON da grade em texto legível no `.fcproj`. Sem definição de efeito: não aparece no painel de efeitos, não é copiada por Colar Atributos e não pode ser removida, ligada/desligada ou resetada pelos comandos `effects.*`. |
| `eval` | Referência na CPU (`apply_cpu`) e passes da GPU (`gpu_passes` + `grade.wgsl`; a LUT vai como imagem 2D auxiliar). Os nós trabalham no sinal: o inverso da decodificação Rec.709 do FilmCraft, então um clipe da FX30 na interpretação padrão chega aos nós com os próprios códigos S-Log3. |
| `cst` | Matemática do nó CST (curvas, matriz, curva fílmica, compressão de gamut). |
| `deltae` | ΔE 2000 (CIEDE2000) entre imagens Rec.709 / Gamma 2.4, para comparar com o DaVinci. |
| `commands` | `koda.color.inspect`, `addNode`, `removeNode`, `moveNode`, `renameNode`, `setNodeEnabled`, `setParam`, `setBypass`, `setGrade`. Cada edição é um passo de desfazer; alvo = `clip` ou o clipe selecionado. |

Nós:

| Tipo | Parâmetros | Notas |
|---|---|---|
| `gain` | `master`, `rgb` (0 a 16) | Marco 1. |
| `cst` | `inputGamut`, `inputCurve`, `outputGamut`, `outputCurve`, `toneMapping` (`none` / `filmic`), `white`, `mid`, `contrast`, `gamutMapping` (`none` / `compress`) | Marco 2. Padrão = CST do powergrade KODA: S-Gamut3.Cine / S-Log3 → Rec.709 / Gamma 2.4 com a curva fílmica por canal. Curva S-Log3 e primárias do `filmcraft-color` (documentos da Sony). |
| `lut` | `lut` (`lib:<id>` da biblioteca de LUTs do projeto), `intensity` (0 a 1), `interpolation` (`trilinear` / `tetrahedral`) | Marco 2. `koda.color.loadLut {node, path}` importa o arquivo (`lut.import`) e liga ao nó. LUTs 1D ou com shaper viram um cubo 33³. |

### Calibração contra o DaVinci (marco 2)

Os algoritmos de tone mapping e gamut mapping do DaVinci não são públicos. A curva fílmica foi
**ajustada pela saída** (caixa-preta, frames 01 → 02 do powergrade do dono): logística em stops,
`código = white / (1 + exp(-(log2(v) - mid) / contrast))` por canal, com white 1,034, mid −1,55,
contrast 1,86. Medido: por canal ΔE 0,68 contra 1,40 na luminância; gamut compress um pouco pior
(0,713 contra 0,706), por isso desligado no padrão. A LUT do nó 05 bate melhor com interpolação
trilinear (ΔE 0,091) que tetraédrica (0,095). Ferramentas: `examples/calibra_cst.rs`,
`examples/calibra_lut.rs`, `examples/imagens_marco2.rs` (precisam de `docs/koda/cor/referencia/`).

## Como se liga ao FilmCraft

`koda_color::register()` (chamado pelos apps na inicialização):

- `filmcraft_render::external`: o render roda a grade **antes** dos outros efeitos do clipe
  (Lumetri incluso) e antes do Motion, na CPU e no plano da GPU, onde a grade vira passes WGSL
  próprios executados pelo estágio de efeitos (`filmcraft-gpu`).
- `filmcraft_engine::commands::register_external`: os comandos entram no registro do motor, então
  a tela, o CLI, o canal de controle e o MCP usam o mesmo caminho.

## Testes

`cargo test -p koda-color`: modelo, sinal, comandos com desfazer, parâmetros hostis, grade oculta e
protegida, ida e volta salvando o projeto, a grade sempre primeiro, GPU contra CPU no compositor
(ganho, CST e LUT), comparação com os frames do DaVinci (ΔE 2000) e comandos de CST e LUT.
Testes que usam `docs/koda/cor/referencia/` (fora do git) são pulados com aviso se os arquivos não
existirem (`koda_color::reference_file`).
