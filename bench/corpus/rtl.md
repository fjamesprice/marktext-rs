# RTL and mixed bidirectional text

MarkText has an upstream RTL fix in its history (`43bd8b77`), so the
Unicode Bidirectional Algorithm is a live requirement, not a theoretical one
(NATIVE-REWRITE-PLAN.md §3 item 3). Every line below mixes scripts, because
pure-RTL text hides exactly the bugs that matter.

## العربية

هذا نص عربي يحتوي على **نص عريض** وكلمة إنجليزية Rust في المنتصف.

- عنصر أول مع رقم 12345
- عنصر ثانٍ مع `code span`
- [رابط](https://example.com/عربي)

## עברית

שורה בעברית עם *הדגשה* ומילה באנגלית parley באמצע המשפט.

> ציטוט בעברית שמכיל 42 ומספרים נוספים 3.14159.

## An image at byte zero

![diagram](./diagram.png) صورة في بداية فقرة عربية، وهذا هو الموضع الذي
يضع فيه محرك النص الصندوق في الطرف الخطأ من السطر.

## Mixed in one paragraph

An English sentence, then مرحبا بالعالم, then back to English, then שלום עולם,
then a number 2026 and a URL https://example.com/mixed — all in one logical
line, which is where caret motion and hit-testing go wrong.

| المفتاح | القيمة |
| --- | ---: |
| الأول | 1 |
| الثاني | 2 |

