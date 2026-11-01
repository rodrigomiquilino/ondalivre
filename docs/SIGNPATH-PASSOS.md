# Passos manuais: SignPath Foundation (OndaLivre)

Este guia lista **somente** o que voce precisa fazer com suas contas. A preparacao
local do repositorio (licenca MIT, `CODE_SIGNING_POLICY.md`, workflow, metadados PE,
configuracao de artefato) ja esta no codigo. **Nao** publique nem crie contas a
partir deste documento automaticamente: execute voce mesmo.

Fontes oficiais:

- Termos: https://signpath.org/terms.html
- Candidatura: https://signpath.org/apply.html
- Conector GitHub: https://docs.signpath.io/trusted-build-systems/github
- Action: https://github.com/signpath/github-action-submit-signing-request
- Configuracao de artefato: https://about.signpath.io/documentation/artifact-configuration

## 1. Conta GitHub e MFA

1. Confirme MFA ativo na sua conta GitHub (Settings → Password and authentication).
2. Crie um repositorio **publico** (ex.: `ondalivre`) sob seu usuario ou organizacao.
3. Adicione o remoto e envie o codigo local (ja com commit inicial):

```bash
cd /caminho/para/ondalivre
git remote add origin https://github.com/SEU_USUARIO/ondalivre.git
git push -u origin main
```

4. Publique ao menos um release ou deixe claro na README o que o app faz e como baixar
   (o SignPath exige projeto documentado e ja liberado na forma a assinar).

## 2. Formulario SignPath Foundation

1. Abra https://signpath.org/apply.html
2. Preencha, em linhas gerais:
   - **Project name:** OndaLivre
   - **Homepage / repository URL:** URL publica do GitHub
   - **License:** MIT (arquivo `LICENSE` na raiz)
   - **Description:** automacao de radio portatil em Rust (UI Fluent); interop de
     listas estilo ZaraRadio Free sem copiar codigo proprietario
   - **Download / Release URL:** pagina de Releases do GitHub (mesmo que ainda
     vazia, indique a URL futura)
   - **CI:** GitHub Actions; aponte para `.github/workflows/release.yml`
   - Mencione que a politica de assinatura esta em `CODE_SIGNING_POLICY.md` e na
     secao **Assinatura de codigo** da README (com a frase obrigatoria de atribuicao)
3. Envie e aguarde a analise. Projetos aceitos aparecem em https://signpath.org/projects

## 3. Apos a aceitacao: app GitHub e projeto SignPath

1. Instale o **SignPath GitHub App** e conceda acesso ao repositorio OndaLivre
   (necessario para o conector e auditoria).
2. No painel SignPath.io, confirme a organizacao OSS e o **project slug** (ex.:
   `ondalivre`).
3. Crie ou confirme a **signing policy** (ex.: `release-signing`). Anote o slug.
4. Em Artifact Configurations, cadastre o XML de
   `.signpath/artifact-configurations/default.xml` (ou faca upload de um
   `ondalivre.exe` de amostra e ajuste para bater com o XML do repo). O slug
   tipico e `default`.
5. Gere um **API token** com permissao de submitter no projeto.

## 4. Secrets e variaveis no GitHub

No repositorio: Settings → Secrets and variables → Actions.

**Secret:**

| Nome | Valor |
| --- | --- |
| `SIGNPATH_API_TOKEN` | token da API SignPath |

**Variables** (repository variables):

| Nome | Valor tipico |
| --- | --- |
| `ORGANIZATION_ID` | UUID da organizacao no SignPath |
| `PROJECT_SLUG` | slug do projeto (ex. `ondalivre`) |
| `SIGNING_POLICY_SLUG` | slug da policy (ex. `release-signing`) |
| `ARTIFACT_CONFIGURATION_SLUG` | opcional; padrao no workflow e `default` |

Os nomes acima sao os que `.github/workflows/release.yml` espera.

## 5. Publicar um tag e assinar

1. No SignPath, esteja pronto para **aprovar manualmente** o pedido de assinatura.
2. Crie e envie um tag de versao (o workflow dispara em `v*`):

```bash
git tag v1.0.0
git push origin v1.0.0
```

3. Acompanhe Actions → workflow **release**:
   - build MSVC `x86_64-pc-windows-msvc`
   - upload do artefato unsigned
   - `signpath/github-action-submit-signing-request@v3`
4. Aprove o signing request no dashboard SignPath.
5. Ao concluir, o job publica `ondalivre.exe` assinado no GitHub Release do tag.

## 6. Checklist rapido de elegibilidade

Conforme https://signpath.org/terms.html:

- [ ] Licenca OSI (MIT) sem dual-licensing comercial
- [ ] Sem codigo proprietario no que sera assinado
- [ ] Repositorio publico, mantido e documentado
- [ ] MFA no GitHub e no SignPath
- [ ] Politica publicada com a frase de atribuicao obrigatoria
- [ ] Metadados PE (ProductName OndaLivre, CompanyName Rodrigo Miquilino, versao)
- [ ] Builds verificaveis so via CI publica; aprovacao manual por release

## O que este repo ja traz pronto

- `LICENSE` (MIT)
- `CODE_SIGNING_POLICY.md` (EN + pt-BR)
- Secao README **Assinatura de codigo**
- `build.rs` + `assets/ondalivre.ico` (VERSIONINFO + icone)
- `.signpath/artifact-configurations/default.xml`
- `.github/workflows/release.yml`
- `.gitignore` sem secrets
