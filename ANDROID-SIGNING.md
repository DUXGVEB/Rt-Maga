# Android release signing

RtMaga direct-download Android releases must be built from a locally stored
release keystore. The keystore and its property file are intentionally ignored
by Git and must never be committed, uploaded, or copied into a release asset.

## One-time setup

Create a PKCS#12 keystore at
`src-tauri/gen/android/rtmaga-release.keystore` using `keytool`. Store it in a
secure backup location controlled by the maintainer. Losing it prevents future
updates from being installed over an existing RtMaga installation.

Create `src-tauri/gen/android/release-signing.properties` with this local-only
content, replacing every placeholder:

```properties
storeFile=rtmaga-release.keystore
storePassword=<keystore-password>
keyAlias=rtmaga-release
keyPassword=<key-password>
```

## Release build and verification

Build the ARM64 APK:

```powershell
npm --prefix ui exec tauri -- android build --target aarch64 --apk --ci
```

Verify the exact generated file before uploading it:

```powershell
$tools = "$env:LOCALAPPDATA\Android\Sdk\build-tools\36.0.0"
& "$tools\apksigner.bat" verify --verbose --print-certs <apk-path>
& "$tools\zipalign.exe" -c -v 4 <apk-path>
```

The release build fails when the local signing property file is missing or
incomplete. Upload only an APK whose `apksigner verify` succeeds.
