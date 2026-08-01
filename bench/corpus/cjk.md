# CJK

`zh-CN`, `zh-TW`, `ja` and `ko` are shipped locales, so CJK correctness is a
requirement rather than a nice-to-have (§7, §10). This file also carries the
CJK flanking cases for `strong`, which `inlineRenderer/__tests__` covers and
which the lexer port must reproduce exactly (§3 rule 3).

## 简体中文

这是一段中文文本，包含**粗体**和*斜体*，以及一个 `代码片段`。

中文**粗体**紧邻中文字符，没有空格——这是 CJK flanking 的关键用例。

## 繁體中文

這是一段繁體中文，包含[連結](https://example.com/中文)與圖片 ![替代文字](./x.png)。

## 日本語

日本語の文章です。**太字**と*斜体*、そして`コード`を含みます。

平仮名、片仮名（カタカナ）、漢字が混在する行。IME の変換途中でも壊れないこと。

## 한국어

한국어 문장입니다. **굵게**와 *기울임*, 그리고 `코드`를 포함합니다.

- 첫 번째 항목
- 두 번째 항목
- 세 번째 항목

## Line breaking

中文没有空格所以断行规则完全依赖UAX14的CJK规则而不是空格这一行故意很长用来触发换行。

| 列一 | 列二 | 列三 |
| --- | :---: | ---: |
| 中文 | 日本語 | 한국어 |
| 值 | 値 | 값 |

