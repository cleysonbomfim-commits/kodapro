# Plano do Koda Pro

> Rascunho inicial (2026-10-15). Revise e ajuste: este plano é do dono do projeto, não do upstream.

O Koda Pro é um fork do [FilmCraft](https://github.com/storytold/filmcraft), editor de vídeo não linear
open-source em Rust puro (licença MIT OR Apache-2.0). O fork vive em
[cleysonbomfim-commits/kodapro](https://github.com/cleysonbomfim-commits/kodapro); o original está no
remote `upstream`.

## Leitura obrigatória no início de cada sessão

1. Este arquivo (`docs/koda/PLANO.md`) e o [diário](DIARIO.md).
2. As regras do upstream continuam valendo: [`AGENTS.md`](../../AGENTS.md) (nunca travar, nada da
   Adobe, código feito do zero) e [`CLAUDE.md`](../../CLAUDE.md).

## Princípios do fork

- **Mudar pouco do upstream.** Trocamos o que o usuário vê (nome, marca); nomes internos (crates,
  binário `filmcraft`, ids de comandos, formato `.fcproj`, pasta de dados) ficam iguais, para que
  dê para puxar atualizações do `upstream` sem conflitos grandes.
- **Créditos e licença.** O aviso de copyright do FilmCraft e os arquivos `LICENSE-*` / `NOTICE` ficam
  como estão.
- **Marca ArtCraft.** Pela licença de marca (`docs/brand/LICENSE-brand.txt`, AGENTS.md §1.8), forks
  modificados devem remover os logos e o nome ArtCraft.
- **Uma etapa por vez:** cada etapa concluída ganha entrada no diário, commit e `git push`.

## Etapas

| # | Etapa | Estado |
|---|---|---|
| 1 | Clonar o repositório, configurar `upstream`, compilar em release e abrir o app no Mac | feito |
| 2 | Trocar o nome exibido de FilmCraft para Koda Pro (janela, menus, diálogos, traduções) | feito |
| 3 | Criar diário e plano (`docs/koda/`) e registrar no `CLAUDE.md` | feito |
| 4 | Remover a marca ArtCraft (logo da janela Sobre, links do Discord e do getartcraft.com) | a fazer |
| 5 | Empacotar `Koda Pro.app` para abrir com dois cliques e mostrar o nome no Dock e na barra de menus | a fazer |
| 6 | Ícone próprio do Koda Pro (original, com arquivo `.attribution`) | a fazer |
| 7 | Rodar o portão completo (`cargo xtask ci`) e corrigir o que falhar | a fazer |
| 8 | Rotina para puxar atualizações do `upstream` (merge periódico) | a fazer |
| 9 | Definir as funcionalidades próprias do Koda Pro | a definir com o dono |

## Como compilar e abrir

```sh
cargo build --release -p filmcraft
./target/release/filmcraft --control 9876
```
