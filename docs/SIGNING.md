# Signing and updates

Three kinds of signing, each switched on by adding secrets to the GitHub
repository (Settings → Secrets and variables → Actions). Until a secret exists,
releases still build, just without that signature.

| What | Removes / enables | Secrets | Status |
|---|---|---|---|
| Update signing | Automatic updates in the app | `TAURI_SIGNING_PRIVATE_KEY` | ✅ set (25 Nov 2026) |
| macOS signing + notarization | "unidentified developer" warning | `APPLE_*` (below) | waiting for Apple Developer enrolment |
| Windows signing (SignPath) | SmartScreen warning | set up after approval | apply once the repo is public |

## 1. Update signing (do this first)

PaperKit only installs updates signed with PaperKit's own key. The key pair
was generated on the maintainer's PC:

- private key: `%USERPROFILE%\.tauri\paperkit-updater.key` (no password)
- public key: in `app/src-tauri/tauri.conf.json` → `plugins.updater.pubkey`

**Back up the private key somewhere safe** (a password manager). If it is lost,
installed copies of PaperKit can no longer be updated automatically: users
would have to reinstall by hand.

Add it as a secret (PowerShell):

```powershell
Get-Content "$env:USERPROFILE\.tauri\paperkit-updater.key" -Raw | gh secret set TAURI_SIGNING_PRIVATE_KEY -R oulismi/Paperkit
```

(Command Prompt: `gh secret set TAURI_SIGNING_PRIVATE_KEY -R oulismi/Paperkit < "%USERPROFILE%\.tauri\paperkit-updater.key"`.)

From then on every release also contains signed update packages and a
`latest.json`, which installed copies check
(`releases/latest/download/latest.json`). That URL only works when the
repository is public. The offline edition doesn't auto-update (an update
package would drop its bundled AI model); it gets a new offline installer.

## 2. macOS: Apple Developer Program

Needs the Apple Developer Program (99 USD/year) in the maintainer's name.

1. Enrol at <https://developer.apple.com/programs/>.
2. In Xcode or on developer.apple.com, create a **Developer ID Application**
   certificate; export it from Keychain Access as a `.p12` with a password.
3. Create an app-specific password at <https://account.apple.com> (Sign-In and
   Security → App-Specific Passwords).
4. Add these secrets:

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | the `.p12`, base64: `base64 -i cert.p12` (macOS) or `certutil -encode` (Windows, strip the header lines) |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | your Apple ID email |
| `APPLE_PASSWORD` | the app-specific password |
| `APPLE_TEAM_ID` | your 10-character Team ID |

The release workflow then signs and notarizes the macOS builds by itself.

## 3. Windows: SignPath Foundation (free for open source)

SignPath signs open-source projects for free, with a certificate issued to
the SignPath Foundation. Requirements include a public repository, an OSI
license (AGPL-3.0 qualifies) and builds made by CI, which PaperKit has.

1. Make the repository public.
2. Apply at <https://signpath.org/apply> with the project URL.
3. After approval SignPath provides an organization ID, project slug, signing
   policy and an API token. Signing then becomes a step in `release.yml`
   (SignPath's GitHub action signs the build artifacts, which are then
   attached to the release). That step can only be written once those values
   exist.

Until then Windows users see SmartScreen once ("More info" → "Run anyway").
