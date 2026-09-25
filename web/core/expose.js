/// 给还没迁成 ES 模块的旧脚本（`window.GqyXxx` 形式）用的公共层出口。
///
/// index.html 里它排在所有旧脚本之前：模块脚本与 defer 脚本按文档顺序执行，
/// 旧脚本求值时 `window.GqyCore` 已经就位。它和 app.js 引用的是同一份模块实例
/// （同一个 URL），所以 401 回调等登记对两边同时生效。旧脚本迁完即可删除本文件。
import { ApiError, apiRequest } from "./api.js";
import { ICONS, SVG_NS } from "./icons.js";
import { showToast } from "./toast.js";
import { deletePath, getPath, setPath } from "./util.js";

window.GqyCore = Object.freeze({ ApiError, ICONS, SVG_NS, apiRequest, deletePath, getPath, setPath, showToast });
