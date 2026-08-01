# Emoji-heavy

Emoji are the cheapest way to break grapheme-cluster handling: ZWJ sequences,
skin-tone modifiers, and regional-indicator flags are all multi-codepoint
single graphemes. Arrow keys must not split them (§5, via parley's cluster
API), and `mt-inline` must not tokenise inside them.

## Simple

😀 😃 😄 😁 😆 😅 🤣 😂 🙂 🙃 😉 😊 😇 🥰 😍 🤩 😘 😗 ☺️ 😚

## ZWJ sequences (one grapheme each)

👨‍👩‍👧‍👦 👩‍👩‍👦 👨‍👨‍👧‍👧 👩‍💻 👨‍🚀 🧑‍🔬 🏳️‍🌈 🏴‍☠️ 👁️‍🗨️

## Skin-tone modifiers

👋🏻 👋🏼 👋🏽 👋🏾 👋🏿 🤝🏽 👨🏿‍🦱 👩🏻‍🦰

## Regional-indicator flags

🇬🇧 🇺🇸 🇯🇵 🇰🇷 🇨🇳 🇹🇼 🇩🇪 🇫🇷 🇪🇸 🇧🇷 🇮🇳 🇸🇦 🇮🇱

## Emoji adjacent to inline syntax

**🎉bold🎉** and *🚀italic🚀* and `🐛code🐛` and [🔗link🔗](https://example.com).

Shortcode form: :smile: :rocket: :+1: — muya lexes these as emoji tokens,
and the word-boundary rule around them is covered by `emojiWordBoundary.spec.ts`.

## Mixed with other scripts

Hello 👋 世界 🌏 مرحبا 🕌 שלום 🕎 — one line, four scripts, five emoji.

| Emoji | Name | Codepoints |
| :---: | --- | --- |
| 👨‍👩‍👧‍👦 | family | 7 |
| 🏳️‍🌈 | rainbow flag | 4 |
| 👋🏿 | wave, dark | 2 |

