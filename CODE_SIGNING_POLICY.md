# Code signing policy / Politica de assinatura de codigo

## English

**Free code signing provided by SignPath.io, certificate by SignPath Foundation.**

### Scope

OndaLivre is an original open-source portable radio automation application (Rust).
It can import playlist/list formats compatible with ZaraRadio Free (`.lst`, `.m3u`,
`.pls`, `.rot`, `.seq`, and `.evt` as documented). It does **not** copy ZaraRadio
source code, icons, or proprietary assets. ZaraRadio remains a product of
ZaraSoft / Kero Systems.

Only binaries built from this public repository via the GitHub Actions release
workflow are submitted for signing.

### Team roles

This is a solo-maintained project. The same person holds all roles:

| Role | Member |
| --- | --- |
| Author / Committer | Rodrigo Miquilino |
| Reviewer | Rodrigo Miquilino |
| Approver (signing requests) | Rodrigo Miquilino |

Every signing request is approved manually in the SignPath dashboard.

### Privacy

This program will not transfer any information to other networked systems unless
specifically requested by the user or the person installing or operating it.

Optional outbound requests that the user may trigger (for example fetching
stream metadata or remote playlist URLs they configured) are under the user's
control. No telemetry or analytics are sent by default.

### Security practices

- Multi-factor authentication is required on the GitHub account and the SignPath
  account used for this project.
- Release binaries are built only by the public GitHub Actions workflow in
  `.github/workflows/release.yml` on GitHub-hosted runners.
- PE metadata (`ProductName`, `CompanyName`, `FileVersion`, `ProductVersion`) is
  embedded at build time and enforced by the SignPath artifact configuration.

### References

- SignPath Foundation terms: https://signpath.org/terms.html
- Apply: https://signpath.org/apply.html
- GitHub Actions connector: https://docs.signpath.io/trusted-build-systems/github
- Artifact configuration: https://about.signpath.io/documentation/artifact-configuration

---

## Portugues (Brasil)

**Free code signing provided by SignPath.io, certificate by SignPath Foundation.**

### Escopo

OndaLivre e um aplicativo original de automacao de radio portatil em codigo aberto
(Rust). Ele pode importar listas no estilo do ZaraRadio Free (`.lst`, `.m3u`,
`.pls`, `.rot`, `.seq` e `.evt` conforme documentado). **Nao** copia codigo-fonte,
icones nem ativos proprietarios do ZaraRadio. O ZaraRadio continua sendo produto
da ZaraSoft / Kero Systems.

Somente binarios gerados a partir deste repositorio publico pelo workflow de
release do GitHub Actions sao enviados para assinatura.

### Papeis da equipe

Projeto mantido por uma unica pessoa. A mesma pessoa acumula todos os papeis:

| Papel | Membro |
| --- | --- |
| Autor / Committer | Rodrigo Miquilino |
| Revisor | Rodrigo Miquilino |
| Aprovador (pedidos de assinatura) | Rodrigo Miquilino |

Cada pedido de assinatura e aprovado manualmente no painel do SignPath.

### Privacidade

Este programa nao transferira nenhuma informacao a outros sistemas em rede, a
menos que isso seja especificamente solicitado pelo usuario ou pela pessoa que
instala ou opera o programa.

Requisicoes de rede opcionais que o usuario possa disparar (por exemplo metadados
de stream ou URLs de playlist remotas que ele configurou) ficam sob controle do
usuario. Nao ha telemetria nem analitica enviada por padrao.

### Praticas de seguranca

- Autenticacao multifator e obrigatoria na conta GitHub e na conta SignPath
  usadas neste projeto.
- Binarios de release sao gerados apenas pelo workflow publico em
  `.github/workflows/release.yml` em runners hospedados pelo GitHub.
- Metadados PE (`ProductName`, `CompanyName`, `FileVersion`, `ProductVersion`)
  sao embutidos na compilacao e reforcados pela configuracao de artefato do
  SignPath.

### Referencias

- Termos da SignPath Foundation: https://signpath.org/terms.html
- Formulario de aplicacao: https://signpath.org/apply.html
- Conector GitHub Actions: https://docs.signpath.io/trusted-build-systems/github
- Configuracao de artefato: https://about.signpath.io/documentation/artifact-configuration
