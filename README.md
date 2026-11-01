# OndaLivre

Automação de rádio portátil em Rust, com interface no estilo Fluent (Windows 11):
painel de navegação, barra de comando, cantos arredondados, tipografia Inter (OFL),
tema claro/escuro e cor de destaque configurável.

O código e a UI são originais. Dá para importar listas texto no estilo do ZaraRadio Free
(`.lst`, `.m3u`, `.pls`, `.rot`, `.seq`) e, nesta versão, também arquivos `.evt` no formato
wxFileConfig (chaves documentadas em `zararadio-study/evidence/round2/INTEROP-RULES.md`).
Não copia código, ícones nem textos do ZaraRadio.

## Rodar (portátil)

- Linux: `./ondalivre` em `dist/linux/`
- Windows: `ondalivre.exe` em `dist/windows/`

Arquivos ao lado do executável: `ondalivre.toml`, `events.json`, `jingles.json`,
`sweepers.json`, `now_playing.txt`, `playlog.csv`, pastas `Time/`, `Temperature/`, `Humidity/`.

### Flags

```text
--load demo.lst
--open-all-windows
--open-window=events|jingles|sweepers|aux|cue|options|find|find_folders|about|postit|logs|main
```

## Build

```bash
cargo build --release
cargo zigbuild --release --target x86_64-pc-windows-gnu
```

## Testes

```bash
cargo test --release
cargo run --release --example smoke_play
```

## Ícones

Ícones de navegação e barra de comando: subset rasterizado (PNG) dos
[Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons)
(Microsoft, licença MIT). Ver `assets/icons/NOTICE.txt`.

## Assinatura de codigo

Free code signing provided by SignPath.io, certificate by SignPath Foundation.

Politica completa (papeis, privacidade, escopo): ver [`CODE_SIGNING_POLICY.md`](CODE_SIGNING_POLICY.md).
Passos que so o mantenedor pode executar (criar repo, candidatar-se, secrets): ver [`docs/SIGNPATH-PASSOS.md`](docs/SIGNPATH-PASSOS.md).

O workflow [`.github/workflows/release.yml`](.github/workflows/release.yml) gera `ondalivre.exe`
(MSVC) no GitHub Actions, envia o artefato ao SignPath e publica o executavel assinado
no GitHub Release do tag.

## Licenca

MIT (OndaLivre). Fluent UI System Icons: MIT (Microsoft). Inter: OFL.
ZaraRadio continua da ZaraSoft / Kero Systems. OndaLivre nao copia codigo do ZaraRadio.
