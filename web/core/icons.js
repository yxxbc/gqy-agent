export const SVG_NS = "http://www.w3.org/2000/svg";

export const ICONS = {
  "arrow-down": [["path", { d: "M12 5v14" }], ["path", { d: "m19 12-7 7-7-7" }]],
  "arrow-up": [["path", { d: "m5 12 7-7 7 7" }], ["path", { d: "M12 19V5" }]],
  atom: [["circle", { cx: "12", cy: "12", r: "1" }], ["path", { d: "M20.2 20.2c2.04-2.03.02-7.37-4.5-11.9-4.52-4.52-9.87-6.54-11.9-4.5-2.04 2.03-.02 7.37 4.5 11.9 4.52 4.52 9.87 6.54 11.9 4.5Z" }], ["path", { d: "M15.7 15.7c4.52-4.52 6.54-9.87 4.5-11.9-2.03-2.04-7.37-.02-11.9 4.5-4.52 4.52-6.54 9.87-4.5 11.9 2.03 2.04 7.37.02 11.9-4.5Z" }]],
  lightbulb: [["path", { d: "M15 14c.2-1 .7-1.7 1.5-2.5 1-.9 1.5-2.2 1.5-3.5A6 6 0 0 0 6 8c0 1 .2 2.2 1.5 3.5.7.7 1.3 1.5 1.5 2.5" }], ["path", { d: "M9 18h6" }], ["path", { d: "M10 22h4" }]],
  brain: [["path", { d: "M9.5 4A2.5 2.5 0 0 1 12 6.5v11a2.5 2.5 0 0 1-4.96.44A2.5 2.5 0 0 1 5.5 13a3 3 0 0 1 .34-5.98A2.5 2.5 0 0 1 9.5 4Z" }], ["path", { d: "M14.5 4A2.5 2.5 0 0 0 12 6.5v11a2.5 2.5 0 0 0 4.96.44A2.5 2.5 0 0 0 18.5 13a3 3 0 0 0-.34-5.98A2.5 2.5 0 0 0 14.5 4Z" }]],
  check: [["path", { d: "M20 6 9 17l-5-5" }]],
  "chevron-down": [["path", { d: "m6 9 6 6 6-6" }]],
  terminal: [["polyline", { points: "4 17 10 11 4 5" }], ["line", { x1: "12", x2: "20", y1: "19", y2: "19" }]],
  target: [["circle", { cx: "12", cy: "12", r: "10" }], ["circle", { cx: "12", cy: "12", r: "6" }], ["circle", { cx: "12", cy: "12", r: "2" }]],
  bot: [["path", { d: "M12 8V4H8" }], ["rect", { x: "4", y: "8", width: "16", height: "12", rx: "2" }], ["path", { d: "M2 14h2" }], ["path", { d: "M20 14h2" }], ["path", { d: "M15 13v2" }], ["path", { d: "M9 13v2" }]],
  "book-open": [["path", { d: "M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" }], ["path", { d: "M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" }]],
  image: [["rect", { x: "3", y: "3", width: "18", height: "18", rx: "2", ry: "2" }], ["circle", { cx: "9", cy: "9", r: "2" }], ["path", { d: "m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" }]],
  smile: [["circle", { cx: "12", cy: "12", r: "10" }], ["path", { d: "M8 14s1.5 2 4 2 4-2 4-2" }], ["line", { x1: "9", x2: "9.01", y1: "9", y2: "9" }], ["line", { x1: "15", x2: "15.01", y1: "9", y2: "9" }]],
  "alarm-clock": [["circle", { cx: "12", cy: "13", r: "8" }], ["path", { d: "M12 9v4l2 2" }], ["path", { d: "M5 3 2 6" }], ["path", { d: "m22 6-3-3" }], ["path", { d: "M6.38 18.7 4 21" }], ["path", { d: "M17.64 18.67 20 21" }]],
  clipboard: [["rect", { x: "8", y: "2", width: "8", height: "4", rx: "1", ry: "1" }], ["path", { d: "M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" }]],
  calculator: [["rect", { x: "4", y: "2", width: "16", height: "20", rx: "2" }], ["line", { x1: "8", x2: "16", y1: "6", y2: "6" }], ["line", { x1: "16", x2: "16", y1: "14", y2: "18" }], ["path", { d: "M16 10h.01" }], ["path", { d: "M12 10h.01" }], ["path", { d: "M8 10h.01" }], ["path", { d: "M12 14h.01" }], ["path", { d: "M8 14h.01" }], ["path", { d: "M12 18h.01" }], ["path", { d: "M8 18h.01" }]],
  search: [["circle", { cx: "11", cy: "11", r: "8" }], ["path", { d: "m21 21-4.3-4.3" }]],
  wallet: [["path", { d: "M19 7V4a1 1 0 0 0-1-1H5a2 2 0 0 0 0 4h15a1 1 0 0 1 1 1v4h-3a2 2 0 0 0 0 4h3a1 1 0 0 0 1-1v-2a1 1 0 0 0-1-1" }], ["path", { d: "M3 5v14a2 2 0 0 0 2 2h15a1 1 0 0 0 1-1v-4" }]],

  puzzle: [["path", { d: "M19.439 7.85c-.049.322.059.648.289.878l1.568 1.568c.47.47.706 1.087.706 1.704s-.235 1.233-.706 1.704l-1.611 1.611a.98.98 0 0 1-.837.276c-.47-.07-.802-.48-.968-.925a2.501 2.501 0 1 0-3.214 3.214c.446.166.855.497.925.968a.979.979 0 0 1-.276.837l-1.61 1.61a2.404 2.404 0 0 1-1.705.707 2.402 2.402 0 0 1-1.704-.706l-1.568-1.568a1.026 1.026 0 0 0-.877-.29c-.493.074-.84.504-1.02.968a2.5 2.5 0 1 1-3.237-3.237c.464-.18.894-.527.967-1.02a1.026 1.026 0 0 0-.289-.877l-1.568-1.568A2.402 2.402 0 0 1 1.998 12c0-.617.236-1.234.706-1.704L4.23 8.77c.24-.24.581-.353.917-.303.515.077.877.528 1.073 1.01a2.5 2.5 0 1 0 3.259-3.259c-.482-.196-.933-.558-1.01-1.073-.05-.336.062-.676.303-.917l1.525-1.525A2.402 2.402 0 0 1 12 1.998c.617 0 1.234.236 1.704.706l1.568 1.568c.23.23.556.338.877.29.493-.074.84-.504 1.02-.968a2.5 2.5 0 1 1 3.237 3.237c-.464.18-.894.527-.967 1.02Z" }]],
  package: [["path", { d: "m7.5 4.27 9 5.15" }], ["path", { d: "M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z" }], ["path", { d: "m3.3 7 8.7 5 8.7-5" }], ["path", { d: "M12 22V12" }]],
  sparkles: [["path", { d: "M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" }], ["path", { d: "M20 3v4" }], ["path", { d: "M22 5h-4" }]],
  code: [["polyline", { points: "16 18 22 12 16 6" }], ["polyline", { points: "8 6 2 12 8 18" }]],
  arch: [["path", { d: "M12 2c-.9 2.3-1.5 3.8-2.6 5.9.7.7 1.5 1.5 2.8 2.4-1.4-.6-2.4-1.2-3.2-1.8C7.5 11.6 5.2 16 2 22c2.9-1.7 5.4-2.8 7.7-3.3.6-2.1 1.4-3.2 2.3-3.2s1.7 1.1 2.3 3.2c2.3.5 4.8 1.6 7.7 3.3-3.2-6-5.5-10.4-7-13.5-.8.6-1.8 1.2-3.2 1.8 1.3-.9 2.1-1.7 2.8-2.4C13.5 5.8 12.9 4.3 12 2z", fill: "currentColor", stroke: "none" }]],
  "chevron-left": [["path", { d: "m15 18-6-6 6-6" }]],
  "layout-grid": [["rect", { x: "3", y: "3", width: "7", height: "7", rx: "1" }], ["rect", { x: "14", y: "3", width: "7", height: "7", rx: "1" }], ["rect", { x: "14", y: "14", width: "7", height: "7", rx: "1" }], ["rect", { x: "3", y: "14", width: "7", height: "7", rx: "1" }]],
  "chart-column": [["path", { d: "M3 3v16a2 2 0 0 0 2 2h16" }], ["path", { d: "M7 15v-4m5 4V8m5 7v-6" }]],
  "chevron-right": [["path", { d: "m9 18 6-6-6-6" }]],
  // 目标状态行用（lucide: target / pause / play / x）
  "target": [["circle", { cx: "12", cy: "12", r: "10" }], ["circle", { cx: "12", cy: "12", r: "6" }], ["circle", { cx: "12", cy: "12", r: "2" }]],
  "pause": [["rect", { x: "14", y: "4", width: "4", height: "16", rx: "1" }], ["rect", { x: "6", y: "4", width: "4", height: "16", rx: "1" }]],
  "play": [["polygon", { points: "6 3 20 12 6 21 6 3" }]],
  "x": [["path", { d: "M18 6 6 18" }], ["path", { d: "m6 6 12 12" }]],
  "undo-2": [["path", { d: "M9 14 4 9l5-5" }], ["path", { d: "M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11" }]],
  "circle-alert": [["circle", { cx: "12", cy: "12", r: "10" }], ["line", { x1: "12", x2: "12", y1: "8", y2: "12" }], ["line", { x1: "12", x2: "12.01", y1: "16", y2: "16" }]],
  "circle-help": [["circle", { cx: "12", cy: "12", r: "10" }], ["path", { d: "M9.09 9a3 3 0 1 1 5.83 1c0 2-3 3-3 3" }], ["path", { d: "M12 17h.01" }]],
  "circle-stop": [["circle", { cx: "12", cy: "12", r: "10" }], ["rect", { width: "6", height: "6", x: "9", y: "9", rx: "1" }]],
  "cloud-sun": [["path", { d: "M12 2v2" }], ["path", { d: "m4.93 4.93 1.41 1.41" }], ["path", { d: "M20 12h2" }], ["path", { d: "m19.07 4.93-1.41 1.41" }], ["path", { d: "M16 6a4 4 0 0 0-3.46 6" }], ["path", { d: "M17.5 19H9a4 4 0 1 1 3.68-5.57A3 3 0 1 1 17.5 19Z" }]],
  copy: [["rect", { width: "14", height: "14", x: "8", y: "8", rx: "2", ry: "2" }], ["path", { d: "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" }]],
  "code-2": [["path", { d: "m18 16 4-4-4-4" }], ["path", { d: "m6 8-4 4 4 4" }], ["path", { d: "m14.5 4-5 16" }]],
  download: [["path", { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }], ["polyline", { points: "7 10 12 15 17 10" }], ["line", { x1: "12", x2: "12", y1: "15", y2: "3" }]],
  "dollar-sign": [["line", { x1: "12", x2: "12", y1: "2", y2: "22" }], ["path", { d: "M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" }]],
  ellipsis: [["circle", { cx: "12", cy: "12", r: "1" }], ["circle", { cx: "19", cy: "12", r: "1" }], ["circle", { cx: "5", cy: "12", r: "1" }]],
  eye: [["path", { d: "M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0" }], ["circle", { cx: "12", cy: "12", r: "3" }]],
  "external-link": [["path", { d: "M15 3h6v6" }], ["path", { d: "M10 14 21 3" }], ["path", { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" }]],
  folder: [["path", { d: "M3 6a2 2 0 0 1 2-2h5l2 2h7a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z" }]],
  globe: [["circle", { cx: "12", cy: "12", r: "10" }], ["path", { d: "M2 12h20" }], ["path", { d: "M12 2a15.3 15.3 0 0 1 0 20" }], ["path", { d: "M12 2a15.3 15.3 0 0 0 0 20" }]],
  "file-text": [["path", { d: "M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z" }], ["polyline", { points: "14 2 14 8 20 8" }], ["line", { x1: "8", x2: "16", y1: "13", y2: "13" }], ["line", { x1: "8", x2: "16", y1: "17", y2: "17" }]],
  "trash-2": [["path", { d: "M3 6h18" }], ["path", { d: "M8 6V4h8v2" }], ["path", { d: "M19 6 18 20H6L5 6" }], ["path", { d: "M10 11v5" }], ["path", { d: "M14 11v5" }]],
  lightbulb: [["path", { d: "M9 18h6" }], ["path", { d: "M10 22h4" }], ["path", { d: "M15.09 14c.18-.59.59-1.05 1.05-1.52A6 6 0 1 0 7.86 12.5c.45.44.85.9 1.03 1.5" }], ["path", { d: "M9 14h6v1a3 3 0 0 1-6 0v-1Z" }]],
  "list-todo": [["rect", { x: "3", y: "5", width: "6", height: "6", rx: "1" }], ["path", { d: "m3 17 2 2 4-4" }], ["path", { d: "M13 6h8" }], ["path", { d: "M13 12h8" }], ["path", { d: "M13 18h8" }]],
  "loader-circle": [["path", { d: "M21 12a9 9 0 1 1-6.219-8.56" }]],
  "log-out": [["path", { d: "M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" }], ["polyline", { points: "16 17 21 12 16 7" }], ["line", { x1: "21", x2: "9", y1: "12", y2: "12" }]],
  ticket: [["path", { d: "M2 9a3 3 0 0 1 0 6v2a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-2a3 3 0 0 1 0-6V7a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2Z" }], ["path", { d: "M13 5v2" }], ["path", { d: "M13 17v2" }], ["path", { d: "M13 11v2" }]],
  user: [["path", { d: "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" }], ["circle", { cx: "12", cy: "7", r: "4" }]],
  "user-plus": [["path", { d: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" }], ["circle", { cx: "9", cy: "7", r: "4" }], ["line", { x1: "19", x2: "19", y1: "8", y2: "14" }], ["line", { x1: "22", x2: "16", y1: "11", y2: "11" }]],
  "lock-keyhole": [["circle", { cx: "12", cy: "16", r: "1" }], ["rect", { x: "3", y: "10", width: "18", height: "12", rx: "2" }], ["path", { d: "M7 10V7a5 5 0 0 1 10 0v3" }]],
  "log-in": [["path", { d: "M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4" }], ["polyline", { points: "10 17 15 12 10 7" }], ["line", { x1: "15", x2: "3", y1: "12", y2: "12" }]],
  "message-circle": [["path", { d: "M21 15a4 4 0 0 1-4 4H8l-5 3V7a4 4 0 0 1 4-4h10a4 4 0 0 1 4 4z" }]],
  "messages-square": [["path", { d: "M14 9a2 2 0 0 1-2 2H6l-4 4V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2z" }], ["path", { d: "M18 9h2a2 2 0 0 1 2 2v10l-4-4h-6a2 2 0 0 1-2-2v-1" }]],
  moon: [["path", { d: "M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" }]],
  "image-search": [["rect", { x: "3", y: "3", width: "14", height: "14", rx: "2" }], ["circle", { cx: "11", cy: "9", r: "2" }], ["path", { d: "m3 15 4-4 5 5" }], ["circle", { cx: "18", cy: "18", r: "3" }], ["path", { d: "m20.2 20.2 1.8 1.8" }]],
  image: [["rect", { x: "3", y: "3", width: "18", height: "18", rx: "2" }], ["circle", { cx: "8.5", cy: "8.5", r: "1.5" }], ["path", { d: "m21 15-5-5L5 21" }]],
  "file-video": [["path", { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }], ["path", { d: "M14 2v6h6" }], ["path", { d: "m10 12.5 4 2.5-4 2.5z" }]],
  "file-audio": [["path", { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }], ["path", { d: "M14 2v6h6" }], ["path", { d: "M15 12v5" }], ["path", { d: "M15 12l-4 1v5" }], ["circle", { cx: "9.5", cy: "18", r: "1.5" }], ["circle", { cx: "13.5", cy: "17", r: "1.5" }]],
  "file-pdf": [["path", { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }], ["path", { d: "M14 2v6h6" }], ["path", { d: "M8 18v-5h1.5a1.5 1.5 0 0 1 0 3H8" }], ["path", { d: "M13 18v-5h1a2 2 0 0 1 0 5z" }], ["path", { d: "M18 13h-2v5" }], ["path", { d: "M16 15.5h1.5" }]],
  "file-archive": [["path", { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }], ["path", { d: "M14 2v6h6" }], ["path", { d: "M9 6h1" }], ["path", { d: "M9 9h1" }], ["path", { d: "M9 12h1" }], ["rect", { x: "8", y: "15", width: "3", height: "4", rx: "1" }]],
  "file-code": [["path", { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }], ["path", { d: "M14 2v6h6" }], ["path", { d: "m10 13-2 2 2 2" }], ["path", { d: "m14 13 2 2-2 2" }]],
  "file-markdown": [["path", { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }], ["path", { d: "M14 2v6h6" }], ["path", { d: "M8 16v-4l2 2 2-2v4" }], ["path", { d: "M15 12v4" }]],
  "file-json": [["path", { d: "M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" }], ["path", { d: "M14 2v6h6" }], ["path", { d: "M8 12h1a1 1 0 0 1 0 2H8v2h1a1 1 0 0 1 0 2H8" }], ["path", { d: "M16 12h-1a1 1 0 0 0 0 2h1v2h-1" }]],
  "maximize-2": [["path", { d: "M15 3h6v6" }], ["path", { d: "m21 3-7 7" }], ["path", { d: "m3 21 7-7" }], ["path", { d: "M9 21H3v-6" }]],
  "minimize-2": [["path", { d: "m14 10 7-7" }], ["path", { d: "M20 10h-6V4" }], ["path", { d: "m3 21 7-7" }], ["path", { d: "M4 14h6v6" }]],
  paintbrush: [["path", { d: "m14.622 17.897-10.68-2.913" }], ["path", { d: "M18.376 2.622a1 1 0 0 1 3.002 3.002L17.36 9.642a2 2 0 0 1-2.121.447l-2.741-1.02a1 1 0 0 1-.583-.583l-1.02-2.741a2 2 0 0 1 .447-2.121Z" }], ["path", { d: "M9 8c-1.804.716-3.5 2.5-3.5 4.5 0 .6.4 1 1 1 2 0 3.784-1.696 4.5-3.5" }]],
  "panel-left": [["rect", { width: "18", height: "18", x: "3", y: "3", rx: "2" }], ["path", { d: "M9 3v18" }]],
  "panel-left-close": [["rect", { width: "18", height: "18", x: "3", y: "3", rx: "2" }], ["path", { d: "M9 3v18" }], ["path", { d: "m15 9-3 3 3 3" }]],
  "panel-left-open": [["rect", { width: "18", height: "18", x: "3", y: "3", rx: "2" }], ["path", { d: "M9 3v18" }], ["path", { d: "m12 9 3 3-3 3" }]],
  "panel-right": [["rect", { width: "18", height: "18", x: "3", y: "3", rx: "2" }], ["path", { d: "M15 3v18" }]],
  paperclip: [["path", { d: "m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48" }]],
  mic: [["path", { d: "M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" }], ["path", { d: "M19 10v2a7 7 0 0 1-14 0v-2" }], ["line", { x1: "12", x2: "12", y1: "19", y2: "22" }]],
  "refresh-cw": [["path", { d: "M21 12a9 9 0 0 0-15.35-6.35L3 8" }], ["path", { d: "M3 3v5h5" }], ["path", { d: "M3 12a9 9 0 0 0 15.35 6.35L21 16" }], ["path", { d: "M16 16h5v5" }]],
  route: [["circle", { cx: "6", cy: "19", r: "3" }], ["path", { d: "M9 19h8.5a3.5 3.5 0 0 0 0-7h-11a3.5 3.5 0 0 1 0-7H15" }], ["circle", { cx: "18", cy: "5", r: "3" }]],
  "settings-2": [["path", { d: "M20 7h-9" }], ["path", { d: "M14 17H5" }], ["circle", { cx: "17", cy: "17", r: "3" }], ["circle", { cx: "7", cy: "7", r: "3" }]],
  "sliders-horizontal": [["line", { x1: "21", x2: "14", y1: "4", y2: "4" }], ["line", { x1: "10", x2: "3", y1: "4", y2: "4" }], ["line", { x1: "21", x2: "12", y1: "12", y2: "12" }], ["line", { x1: "8", x2: "3", y1: "12", y2: "12" }], ["line", { x1: "21", x2: "16", y1: "20", y2: "20" }], ["line", { x1: "12", x2: "3", y1: "20", y2: "20" }], ["line", { x1: "14", x2: "14", y1: "2", y2: "6" }], ["line", { x1: "8", x2: "8", y1: "10", y2: "14" }], ["line", { x1: "16", x2: "16", y1: "18", y2: "22" }]],
  sparkles: [["path", { d: "m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z" }], ["path", { d: "M5 3v4" }], ["path", { d: "M19 17v4" }], ["path", { d: "M3 5h4" }], ["path", { d: "M17 19h4" }]],
  smile: [["circle", { cx: "12", cy: "12", r: "9" }], ["path", { d: "M8 14s1.5 2 4 2 4-2 4-2" }], ["path", { d: "M9 9h.01" }], ["path", { d: "M15 9h.01" }]],
  "stop-square": [["rect", { x: "6", y: "6", width: "12", height: "12", rx: "2", fill: "currentColor", stroke: "none" }]],
  "square-pen": [["path", { d: "M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" }], ["path", { d: "M18.37 2.63a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4Z" }]],
  sun: [["circle", { cx: "12", cy: "12", r: "4" }], ["path", { d: "M12 2v2" }], ["path", { d: "M12 20v2" }], ["path", { d: "m4.93 4.93 1.42 1.42" }], ["path", { d: "m17.66 17.66 1.41 1.41" }], ["path", { d: "M2 12h2" }], ["path", { d: "M20 12h2" }], ["path", { d: "m6.34 17.66-1.41 1.41" }], ["path", { d: "m19.07 4.93-1.41 1.41" }]],
  "sun-moon": [["path", { d: "M12 8a2.83 2.83 0 0 0 4 4 4 4 0 1 1-4-4" }], ["path", { d: "M12 2v2" }], ["path", { d: "M12 20v2" }], ["path", { d: "m4.9 4.9 1.4 1.4" }], ["path", { d: "m17.7 17.7 1.4 1.4" }], ["path", { d: "M2 12h2" }], ["path", { d: "M20 12h2" }], ["path", { d: "m6.3 17.7-1.4 1.4" }], ["path", { d: "m19.1 4.9-1.4 1.4" }]],
  "triangle-alert": [["path", { d: "m21.73 18-8-14a2 2 0 0 0-3.46 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" }], ["path", { d: "M12 9v4" }], ["path", { d: "M12 17h.01" }]],
  wrench: [["path", { d: "M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94z" }]],
  "zoom-in": [["circle", { cx: "11", cy: "11", r: "8" }], ["path", { d: "m21 21-4.3-4.3" }], ["path", { d: "M11 8v6" }], ["path", { d: "M8 11h6" }]],
  "zoom-out": [["circle", { cx: "11", cy: "11", r: "8" }], ["path", { d: "m21 21-4.3-4.3" }], ["path", { d: "M8 11h6" }]],
  x: [["path", { d: "M18 6 6 18" }], ["path", { d: "m6 6 12 12" }]]
};

export function createIcon(name, className = "") {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  if (className) svg.setAttribute("class", className);
  const definition = ICONS[name] || ICONS["circle-alert"];
  for (const [tag, attributes] of definition) {
    const node = document.createElementNS(SVG_NS, tag);
    for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, value);
    svg.appendChild(node);
  }
  return svg;
}

export function renderIconSlots(root = document) {
  const slots = [];
  if (root instanceof Element && root.matches("[data-icon]")) slots.push(root);
  slots.push(...root.querySelectorAll("[data-icon]"));
  for (const slot of slots) {
    slot.replaceChildren(createIcon(slot.dataset.icon));
  }
}

export function makeIconSlot(name, className = "") {
  const slot = document.createElement("span");
  slot.className = `icon-slot${className ? ` ${className}` : ""}`;
  // 图标名留在 DOM 上:走查要断言「视频附件用的是视频图标」,不然只能比 SVG
  // 路径字符串,那是一读就废的测试。
  slot.dataset.icon = name;
  slot.setAttribute("aria-hidden", "true");
  slot.appendChild(createIcon(name));
  return slot;
}
