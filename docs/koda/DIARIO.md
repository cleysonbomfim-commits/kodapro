# Diário do Koda Pro

Uma entrada por etapa concluída, da mais recente para a mais antiga. Cada entrada diz o que foi feito,
o que funcionou, o que deu problema e qual o próximo passo. O plano está em [PLANO.md](PLANO.md).

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
