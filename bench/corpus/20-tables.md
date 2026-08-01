# 20 tables

Tables need two layout passes (§5): measure natural column widths, then
distribute available width. This is the input that makes that expensive.

## Table 0

| Left | Centre | Right |
| :--- | :---: | ---: |
| dirty | **trip** | 79602 |
| viewport | **decoration** | 52790 |
| decoration | **widget** | 27989 |
| render | **decoration** | 70507 |
| conformance | **editor** | 94036 |
| codepoint | **widget** | 65612 |
| viewport | **paragraph** | 22858 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 1

| Left | Centre | Right |
| --- | :--- | ---: |
| codepoint | **ligature** | 95446 |
| fence | **atlas** | 50904 |
| arena | **marker** | 7021 |
| round | **widget** | 54629 |
| render | **atlas** | 23321 |
| markdown | **reveal** | 36896 |
| scroll | **conformance** | 4882 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 2

| Left | Centre | Right |
| :---: | :---: | :---: |
| conformance | **widget** | 33879 |
| conformance | **rope** | 81998 |
| round | **reveal** | 91396 |
| lossless | **lossless** | 71830 |
| heading | **widget** | 9127 |
| atlas | **budget** | 941 |
| atlas | **reveal** | 30082 |
| serializer | **document** | 23475 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 3

| Left | Centre | Right |
| :--- | :---: | ---: |
| glyph | **scroll** | 36511 |
| ligature | **shaping** | 67955 |
| fence | **rope** | 1431 |
| document | **scroll** | 41045 |
| latency | **editor** | 32468 |
| reflow | **widget** | 84938 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 4

| Left | Centre | Right |
| --- | :--- | ---: |
| caret | **codepoint** | 42521 |
| cluster | **revision** | 18097 |
| fence | **layout** | 89025 |
| throughput | **verbatim** | 90843 |
| reveal | **widget** | 71962 |
| round | **offset** | 29768 |
| cluster | **throughput** | 18411 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 5

| Left | Centre | Right |
| :---: | :---: | :---: |
| offset | **serializer** | 58654 |
| bidi | **revision** | 18542 |
| selection | **fence** | 57715 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 6

| Left | Centre | Right |
| :--- | :---: | ---: |
| rope | **ligature** | 61107 |
| shaping | **fixture** | 69415 |
| encoding | **reflow** | 77443 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 7

| Left | Centre | Right |
| --- | :--- | ---: |
| widget | **markdown** | 11018 |
| selection | **latency** | 3429 |
| bidi | **cluster** | 24363 |
| ratchet | **reflow** | 93246 |
| token | **marker** | 74116 |
| serializer | **offset** | 26500 |
| glyph | **arena** | 72940 |
| lossless | **token** | 86095 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 8

| Left | Centre | Right |
| :---: | :---: | :---: |
| grapheme | **dirty** | 52218 |
| codepoint | **widget** | 28503 |
| trip | **token** | 17273 |
| conformance | **glyph** | 13893 |
| caret | **budget** | 38995 |
| rope | **bidi** | 71751 |
| ligature | **buffer** | 66951 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 9

| Left | Centre | Right |
| :--- | :---: | ---: |
| reveal | **token** | 45178 |
| incremental | **fence** | 28322 |
| trip | **heading** | 77705 |
| invariant | **widget** | 93701 |
| reflow | **encoding** | 3500 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 10

| Left | Centre | Right |
| --- | :--- | ---: |
| atlas | **offset** | 51956 |
| range | **cluster** | 11931 |
| layout | **range** | 6300 |
| marker | **shaping** | 77015 |
| raster | **editor** | 28637 |
| scroll | **atlas** | 37004 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 11

| Left | Centre | Right |
| :---: | :---: | :---: |
| caret | **grapheme** | 49215 |
| reflow | **glyph** | 58593 |
| paragraph | **latency** | 35514 |
| range | **shaping** | 12747 |
| rope | **cluster** | 88737 |
| cluster | **round** | 56840 |
| cluster | **marker** | 82250 |
| widget | **revision** | 86436 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 12

| Left | Centre | Right |
| :--- | :---: | ---: |
| throughput | **baseline** | 34340 |
| fence | **incremental** | 64558 |
| trip | **caret** | 62065 |
| paragraph | **cluster** | 60516 |
| serializer | **render** | 48334 |
| cluster | **latency** | 17633 |
| viewport | **heading** | 37305 |
| bidi | **revision** | 25215 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 13

| Left | Centre | Right |
| --- | :--- | ---: |
| ratchet | **verbatim** | 60153 |
| selection | **codepoint** | 89898 |
| glyph | **verbatim** | 74779 |
| encoding | **widget** | 5177 |
| marker | **reveal** | 66919 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 14

| Left | Centre | Right |
| :---: | :---: | :---: |
| reveal | **decoration** | 97445 |
| widget | **latency** | 63442 |
| trip | **editor** | 39765 |
| glyph | **revision** | 81815 |
| codepoint | **atlas** | 43591 |
| token | **serializer** | 4941 |
| serializer | **rope** | 19555 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 15

| Left | Centre | Right |
| :--- | :---: | ---: |
| round | **widget** | 91754 |
| codepoint | **raster** | 44246 |
| grapheme | **invariant** | 62251 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 16

| Left | Centre | Right |
| --- | :--- | ---: |
| bidi | **shaping** | 81387 |
| arena | **inline** | 28559 |
| trip | **grapheme** | 74033 |
| arena | **bidi** | 80726 |
| reflow | **document** | 61433 |
| baseline | **encoding** | 61569 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 17

| Left | Centre | Right |
| :---: | :---: | :---: |
| ratchet | **incremental** | 38453 |
| atlas | **budget** | 42358 |
| conformance | **token** | 50552 |
| shaping | **grapheme** | 66431 |
| paragraph | **heading** | 38688 |
| offset | **incremental** | 18613 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 18

| Left | Centre | Right |
| :--- | :---: | ---: |
| ratchet | **atlas** | 78952 |
| reflow | **baseline** | 71255 |
| scroll | **glyph** | 35681 |
| grapheme | **buffer** | 7451 |
| budget | **cluster** | 65616 |
| a \| b | `x \| y` | [link](https://example.com) |

## Table 19

| Left | Centre | Right |
| --- | :--- | ---: |
| ratchet | **layout** | 25096 |
| decoration | **render** | 60412 |
| conformance | **selection** | 36681 |
| reveal | **bidi** | 69265 |
| caret | **reflow** | 18584 |
| ligature | **widget** | 92520 |
| a \| b | `x \| y` | [link](https://example.com) |

