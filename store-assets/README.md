# Store listings

Everything Google Play and App Store Connect ask for, apart from what only the account holder can do
(their accounts, keys and forms, listed at the end). The text and image folders follow fastlane's
layouts (`supply` for Play, `deliver` for the App Store), so an upload can be automated later; nothing
here needs fastlane.

The requirements were checked against Google's and Apple's own pages on 20 November 2026.

```
store-assets/
  icon/                      the icon's sources (icon-full.svg, icon-fg.svg) and the 1024 px iOS icon
  android/en-US/
    title.txt                ≤ 30 characters
    short_description.txt    ≤ 80
    full_description.txt     ≤ 4,000
    images/icon.png          512 × 512, the hi-res icon
    images/featureGraphic.png  1024 × 500, no alpha
    images/phoneScreenshots/ 1080 × 1920 (9:16, what Play wants for its recommendations), no alpha
  ios/
    metadata/en-US/          name, subtitle (≤ 30 each), promotional_text (≤ 170), description
                             (≤ 4,000), keywords (≤ 100 bytes), support, marketing and privacy URLs
    metadata/                copyright, primary and secondary category
    metadata/review_information/notes.txt   what the reviewer is told
    screenshots/en-US/       1320 × 2868 (the 6.9-inch iPhone set; the smaller sizes are scaled from
                             it), no alpha
```

## Making them again

- **Screenshots and the feature graphic:** `mise run store:screenshots` builds both sites and runs
  `scripts/store-screenshots.mjs`, which drives the page in Chromium into each scene, photographs it
  at a phone's size and sets it under its caption. The scenes, captions and sizes are at the top of
  that script. The iOS set is shot from the iOS build of the page (no neural voices), the Android set
  from the full one; the reading scene stands in a fake of the app's voice list, since a browser has
  no phone voices.
- **Icons:** `node scripts/generate-icons.mjs` (needs the Tauri CLI) makes every platform's icons
  from `icon/`, the 1024 px iOS icon without alpha, and Play's 512 px one.
- **The privacy policy and support pages** are `web/public/privacy.html` and
  `web/public/support.html`. They ship in both apps (about → "privacy" opens the bundled copy,
  offline) and are served at <https://prosody.rocks/privacy> and <https://prosody.rocks/support>
  once deployed. **Both stores check those URLs, so deploy before submitting**: they come with
  this branch and are not live yet.

## The builds

Each upload needs a build number higher than the last that store has seen: `PROSODY_BUILD`. The
version both stores show is `tauri.conf.json`'s (`0.1.0`).

**Android:** `PROSODY_BUILD=<n> mise run android:bundle` writes the release App Bundle
(`src-tauri/gen/android/app/build/outputs/bundle/universalRelease/app-universal-release.aab`), arm64
only, signed with the upload key. It checks that the bundle is signed and that the native library's
segments are 16 KB-aligned, which Play requires of every update from 1 February 2027. The
upload key is made once and kept out of the repository; Play App Signing holds the key that signs what
phones install.

```sh
keytool -genkeypair -v -keystore ~/keys/prosody-upload.jks -storetype JKS -keyalg RSA -keysize 2048 \
  -validity 10000 -alias upload
# then src-tauri/gen/android/keystore.properties (gitignored):
storeFile=/Users/<you>/keys/prosody-upload.jks
password=<the store password>
keyAlias=upload
```

Back the keystore and its password up somewhere safe. A lost upload key can be reset through Play
support, but only with some trouble.

**iOS:** `PROSODY_BUILD=<n> mise run ios:archive` (with `APPLE_DEVELOPMENT_TEAM` set, for example in
`mise.local.toml`) builds the release archive and exports `target/ios/release/Prosody.ipa` for the
App Store. The export makes the distribution profile, signed in either as an Apple account in
Xcode › Settings › Accounts, or with an App Store Connect API key (`PROSODY_ASC_KEY`, the `.p8`
file, with `PROSODY_ASC_KEY_ID` and `PROSODY_ASC_ISSUER`). Upload the `.ipa` with Transporter or
`xcrun altool --upload-app`.

What the iOS build declares, so App Store Connect asks less:

- **iPhone only** (`TARGETED_DEVICE_FAMILY` 1). An iPad runs it as an iPhone app, and no iPad
  screenshots are needed.
- **`ITSAppUsesNonExemptEncryption` = NO.** The app links no TLS library, and WebKit's HTTPS is
  exempt, so no export questions are asked at upload.
- **A privacy manifest** (`prosody-app_iOS/PrivacyInfo.xcprivacy`): no tracking, no data collected,
  and the two required-reason APIs the app's own code calls (file timestamps, C617.1; system boot
  time, 35F9.1), found by reading the release library's symbols. Check it again when a dependency is
  added.

## Google Play Console: what to enter

**Create the app.** Name "Prosody: Poems into Music", default language English (United States),
App, Free.

**Store listing.** Paste the text files above and upload the images. Then:

- Category: Music & Audio. Tags: whatever fits among Music, Poetry or Education.
- Contact email: me@prosody.rocks. Website: <https://prosody.rocks>.

**App content**, the policy forms:

| Form | Answer |
|---|---|
| Privacy policy | <https://prosody.rocks/privacy> |
| Ads | No ads |
| App access | All functionality is available without special access |
| Content rating (IARC) | Category: all other app types. No violence, sexual content, profanity, drugs, gambling or crude humour; users do not interact or share content inside the app; no location sharing; no purchases. Expect Everyone / PEGI 3. |
| Target audience | 13 and over. Choosing under 13 brings in the Families policy; the app would likely meet it, but that is a decision to make on purpose. |
| News app | No |
| Data safety | **No data collected, no data shared.** The app sends nothing off the device. The text the system voice reads goes to the phone's own speech engine, not to the app's servers. The neural voices are downloaded from jsDelivr and Hugging Face only when chosen, after a prompt that gives their size, and no user data goes with the request. |
| Advertising ID | Not used |
| Foreground service | **mediaPlayback** (the app declares it). The form asks what the feature is ("plays the music the poem became, in the background, after the user presses Play"), the effect of an interruption (the music stops), and **a link to a video** showing a user starting playback and leaving the app. Record one on the phone. |
| Government, financial and health features | None |

**Testing.** A personal developer account made after 13 November 2023 must run a **closed test with
at least 12 testers, opted in for 14 days in a row**, before it can apply for production access. Start
it early.

**Release.** Use Play App Signing (the default), upload the `.aab` and pick the countries.

## App Store Connect: what to enter

**Create the app.** Platform iOS, name "Prosody: Poems into Music", primary language English (U.S.),
bundle ID `rocks.prosody` (registered when Xcode first signs a release, or under Certificates,
Identifiers & Profiles), SKU `prosody`.

**App Information:**

- Subtitle: from `subtitle.txt`.
- Category: Music, with Education as the secondary category.
- Content rights: "Yes, it contains third-party content, and I have the rights". That covers the
  public-domain poems, the soundfonts and drums under their licences, and the CC BY reading voice.
  The licences dialog credits every one.
- Age rating: answer **None** or **No** to every question, including Unrestricted Web Access (No:
  the web view loads only the app's own page) and the November 2026 "Shared Location" and "Dating"
  questions. Expect 4+.

**App Privacy:** "Data Not Collected". Privacy policy URL <https://prosody.rocks/privacy>.

**Pricing and Availability:** Free, all countries. For the EU, the Digital Services Act asks whether
you are a trader. A trader's address and phone number are shown on the listing; decide this before
EU release.

**The version page:**

- Upload the screenshots and paste the text: promotional text, description, keywords, and the
  support and marketing URLs.
- Copyright: from `copyright.txt`.
- Pick the build uploaded by `ios:archive`.
- **App Review information:** no sign-in required. Paste the notes from
  `review_information/notes.txt`, and give a contact's name, phone and email for the reviewer.

## Only you can do these

**Accounts and keys:**

- [ ] A Google Play developer account, and an Apple Developer Program membership with an App Store
      Connect record for `rocks.prosody`.
- [ ] The Android upload key and `keystore.properties`.
- [ ] Xcode signed in as your account, or an App Store Connect API key.

**Before submitting:**

- [ ] Deploy the site, so the privacy and support URLs answer. Each store checks them.
- [ ] Check that "Prosody: Poems into Music" is free on both stores. The name has to be unique on the
      App Store.
- [ ] Record the foreground-service video for Play, and start the 14-day closed test.
- [ ] Try the lock-screen controls and an interruption (a call or an alarm) on an iPhone. Neither
      has been tried on a real phone yet (`docs/player.md` §14).
- [ ] Decide your EU trader status.

**Coming later:** from April 2027, App Store Connect will also want screenshots for iPhone Duo
(1398 × 2034 outer, 2007 × 2853 inner) for apps built with the iOS 27.1 SDK or later.
