# koda-color — aba Cor do KODA Pro

Grades de cor por clipe, feitas de nós em série, no estilo do DaVinci Resolve. Especificação:
[`docs/koda/cor/ESPEC_COR_V1.md`](../../docs/koda/cor/ESPEC_COR_V1.md).

## O que tem aqui (marco 1)

| Módulo | O que faz |
|---|---|
| `grade` | Modelo: `Grade` (lista ordenada de `Node`, bypass, ids estáveis). Tudo que vem de projeto, powergrade ou comando é sanitizado (máx. 64 nós, nomes até 64 caracteres, parâmetros dentro da faixa, nunca NaN). |
| `store` | A grade fica no clipe como efeito oculto `koda_grade`, com o JSON da grade em texto legível no `.fcproj`. Sem definição de efeito: não aparece no painel de efeitos, não é copiada por Colar Atributos e não pode ser removida, ligada/desligada ou resetada pelos comandos `effects.*`. |
| `eval` | Referência na CPU (`apply_cpu`) e passes da GPU (`gpu_passes` + `grade.wgsl`). Os nós trabalham no sinal (sRGB/Rec.709 estendido, sem cortar valores), não em luz linear. |
| `commands` | `koda.color.inspect`, `addNode`, `removeNode`, `moveNode`, `renameNode`, `setNodeEnabled`, `setParam`, `setBypass`, `setGrade`. Cada edição é um passo de desfazer; alvo = `clip` ou o clipe selecionado. |

Nós no marco 1: **Ganho** (`gain`: `master` e `rgb`, 0 a 16).

## Como se liga ao FilmCraft

`koda_color::register()` (chamado pelos apps na inicialização):

- `filmcraft_render::external`: o render roda a grade **antes** dos outros efeitos do clipe
  (Lumetri incluso) e antes do Motion, na CPU e no plano da GPU, onde a grade vira passes WGSL
  próprios executados pelo estágio de efeitos (`filmcraft-gpu`).
- `filmcraft_engine::commands::register_external`: os comandos entram no registro do motor, então
  a tela, o CLI, o canal de controle e o MCP usam o mesmo caminho.

## Testes

`cargo test -p koda-color`: modelo, sinal, comandos com desfazer, parâmetros hostis, grade oculta e
protegida, ida e volta salvando o projeto, a grade sempre primeiro, GPU contra CPU no compositor.
Testes que usam `docs/koda/cor/referencia/` (fora do git) são pulados com aviso se os arquivos não
existirem (`koda_color::reference_file`).
