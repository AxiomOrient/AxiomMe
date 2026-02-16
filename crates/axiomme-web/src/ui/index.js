const uriInput = document.getElementById("uri");
const editor = document.getElementById("editor");
const viewer = document.getElementById("viewer");
const statusEl = document.getElementById("status");
const loadBtn = document.getElementById("load");
const reloadBtn = document.getElementById("reload");

const docTitleEl = document.getElementById("doc-title");
const docUriEl = document.getElementById("doc-uri");
const docModeEl = document.getElementById("doc-mode");
const docEditBtn = document.getElementById("doc-edit");
const docSaveBtn = document.getElementById("doc-save");
const docCancelBtn = document.getElementById("doc-cancel");

const treeRootInput = document.getElementById("tree-root");
const treeLoadBtn = document.getElementById("tree-load");
const rootPresetButtons = Array.from(document.querySelectorAll(".preset-root"));
const mkdirBtn = document.getElementById("fs-mkdir");
const moveBtn = document.getElementById("fs-move");
const deleteBtn = document.getElementById("fs-delete");
const treeEl = document.getElementById("tree");
const selectionEl = document.getElementById("selection");
const showGeneratedInput = document.getElementById("show-generated");

let currentUri = null;
let currentEtag = null;
let currentFormat = "text";
let currentEditable = false;
let loadedContent = "";
let selectedUri = null;
let selectedIsDir = false;
let showGenerated = false;
let isEditing = false;
const collapsedDirs = new Set();
const knownDirs = new Set();

function setStatus(kind, message) {
  statusEl.dataset.kind = kind;
  statusEl.textContent = message;
}

function parseError(payload) {
  if (!payload || !payload.code) {
    return { code: "UNKNOWN", message: "unknown error" };
  }
  return payload;
}

function escapeHtml(input) {
  return (input || "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function shortName(uri) {
  if (!uri) return "(none)";
  const idx = uri.lastIndexOf("/");
  if (idx <= uri.indexOf("://") + 1) {
    return uri.replace("axiom://", "");
  }
  return uri.slice(idx + 1) || uri;
}

function parentUri(uri) {
  if (!uri) return null;
  const schemeIdx = uri.indexOf("://");
  const lastSlash = uri.lastIndexOf("/");
  if (schemeIdx === -1 || lastSlash <= schemeIdx + 2) {
    return uri;
  }
  return uri.slice(0, lastSlash);
}

function normalizeRootUri(uri) {
  return (uri || "").trim().replace(/\/+$/, "");
}

function joinUri(base, segment) {
  const clean = (segment || "").trim().replaceAll("/", "");
  if (!clean) return null;
  return `${base.replace(/\/$/, "")}/${clean}`;
}

function isGeneratedTierName(name) {
  return name === ".abstract.md" || name === ".overview.md" || name === ".meta.json";
}

function isVisibleTreeNode(node) {
  if (showGenerated) return true;
  if (!node || node.is_dir) return true;
  return !isGeneratedTierName(shortName(node.uri));
}

function setSelection(uri, isDir) {
  selectedUri = uri || null;
  selectedIsDir = !!isDir;
  if (!selectedUri) {
    selectionEl.textContent = "Selected: (none)";
    return;
  }
  selectionEl.textContent = `Selected ${selectedIsDir ? "directory" : "file"}: ${selectedUri}`;
  highlightSelectedNode();
}

function setDocumentIdentity(uri) {
  if (!uri) {
    docTitleEl.textContent = "No Document Selected";
    docUriEl.textContent = "Select a file from the filesystem.";
    return;
  }
  docTitleEl.textContent = shortName(uri);
  docUriEl.textContent = uri;
}

function setDocumentMode(mode) {
  isEditing = mode === "edit";
  document.body.dataset.docMode = isEditing ? "edit" : "view";
  if (isEditing) {
    docModeEl.textContent = `Editing ${currentFormat.toUpperCase()}`;
    editor.focus();
  } else {
    if (!currentUri) {
      docModeEl.textContent = "Viewer mode";
    } else if (!currentEditable) {
      docModeEl.textContent = `Viewer (read-only ${currentFormat.toUpperCase()})`;
    } else {
      docModeEl.textContent = `Viewer ${currentFormat.toUpperCase()}`;
    }
  }
}

function updateEditorControls() {
  docEditBtn.classList.remove("readonly");
  if (!currentUri) {
    docEditBtn.textContent = "Edit";
    return;
  }
  if (!currentEditable) {
    docEditBtn.textContent = "Read-only";
    docEditBtn.classList.add("readonly");
    return;
  }
  docEditBtn.textContent = "Edit";
}

function isDirty() {
  return isEditing && editor.value !== loadedContent;
}

function renderReadonlyDocument(text, format) {
  const label = escapeHtml((format || "text").toUpperCase());
  const payload = escapeHtml(text || "");
  return `<h3>${label}</h3><pre><code>${payload}</code></pre>`;
}

async function renderViewer(content, format) {
  if (!currentUri) {
    viewer.innerHTML = "<p>Select a file to view.</p>";
    return;
  }

  if (format !== "markdown") {
    viewer.innerHTML = renderReadonlyDocument(content, format);
    return;
  }

  const resp = await fetch("/api/markdown/preview", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ content }),
  });
  const body = await resp.json();
  if (!resp.ok) {
    viewer.innerHTML = renderReadonlyDocument(content, format);
    setStatus("error", "Preview render failed. Showing raw content.");
    return;
  }
  viewer.innerHTML = body.html || "";
}

function highlightSelectedNode() {
  const buttons = treeEl.querySelectorAll(".tree-node");
  for (const button of buttons) {
    button.classList.toggle("selected", button.dataset.uri === selectedUri);
  }
}

function rememberTreeDirectories(node) {
  if (!node || !node.is_dir) {
    return;
  }

  if (!knownDirs.has(node.uri)) {
    knownDirs.add(node.uri);
    collapsedDirs.add(node.uri);
  }

  if (Array.isArray(node.children)) {
    for (const child of node.children) {
      rememberTreeDirectories(child);
    }
  }
}

function updateRootPresetState(rootUri) {
  const normalized = normalizeRootUri(rootUri);
  for (const button of rootPresetButtons) {
    const target = normalizeRootUri(button.dataset.root || "");
    const isActive = target === normalized;
    button.dataset.active = isActive ? "true" : "false";
    button.setAttribute("aria-pressed", isActive ? "true" : "false");
  }
}

function renderTree(root) {
  treeEl.innerHTML = "";
  if (!root || !root.uri) {
    return;
  }

  const fragment = document.createDocumentFragment();

  function walk(node, depth) {
    if (!isVisibleTreeNode(node)) {
      return;
    }

    const row = document.createElement("div");
    row.className = "tree-row";
    row.style.paddingLeft = `${8 + depth * 13}px`;

    const hasChildren = !!(node.is_dir && Array.isArray(node.children) && node.children.length > 0);
    const isCollapsed = collapsedDirs.has(node.uri);

    const toggle = document.createElement("button");
    toggle.type = "button";
    toggle.className = "tree-toggle";

    if (hasChildren) {
      toggle.textContent = isCollapsed ? "▸" : "▾";
      toggle.setAttribute("aria-label", isCollapsed ? "Expand folder" : "Collapse folder");
      toggle.addEventListener("click", (event) => {
        event.stopPropagation();
        if (collapsedDirs.has(node.uri)) {
          collapsedDirs.delete(node.uri);
        } else {
          collapsedDirs.add(node.uri);
        }
        renderTree(root);
      });
    } else {
      toggle.textContent = "▸";
      toggle.disabled = true;
      toggle.classList.add("placeholder");
    }

    const button = document.createElement("button");
    button.type = "button";
    button.className = "tree-node";
    button.dataset.uri = node.uri;
    button.dataset.dir = node.is_dir ? "1" : "0";

    const badge = document.createElement("span");
    badge.className = "tree-badge";
    badge.textContent = node.is_dir ? "[D]" : "[F]";

    const label = document.createElement("span");
    label.textContent = shortName(node.uri);

    button.appendChild(badge);
    button.appendChild(label);
    button.addEventListener("click", async () => {
      setSelection(node.uri, !!node.is_dir);
      if (!node.is_dir) {
        uriInput.value = node.uri;
        await loadDocument();
      }
    });

    row.appendChild(toggle);
    row.appendChild(button);
    fragment.appendChild(row);

    if (node.is_dir && !isCollapsed && Array.isArray(node.children)) {
      for (const child of node.children) {
        walk(child, depth + 1);
      }
    }
  }

  walk(root, 0);
  treeEl.appendChild(fragment);
  highlightSelectedNode();
}

async function loadTree() {
  const root = treeRootInput.value.trim();
  if (!root) {
    setStatus("error", "Tree root URI is required.");
    return;
  }

  updateRootPresetState(root);
  setStatus("saving", "Loading tree...");
  const resp = await fetch(`/api/fs/tree?uri=${encodeURIComponent(root)}`);
  const body = await resp.json();
  if (!resp.ok) {
    const err = parseError(body);
    setStatus("error", `${err.code}: ${err.message}`);
    return;
  }

  rememberTreeDirectories(body.root);
  renderTree(body.root);
  setStatus("saved", `Tree: ${root}`);
}

async function confirmDiscardIfDirty() {
  if (!isDirty()) return true;
  return window.confirm("Unsaved changes exist. Discard and continue?");
}

async function loadDocument() {
  const uri = uriInput.value.trim();
  if (!uri) {
    setStatus("error", "URI is required.");
    return;
  }

  if (!(await confirmDiscardIfDirty())) {
    return;
  }

  setStatus("saving", "Loading document...");
  const resp = await fetch(`/api/document?uri=${encodeURIComponent(uri)}`);
  const body = await resp.json();
  if (!resp.ok) {
    const err = parseError(body);
    if (resp.status === 423) {
      setStatus("locked", "Locked: save/reindex in progress.");
    } else {
      setStatus("error", `${err.code}: ${err.message}`);
    }
    return;
  }

  currentUri = body.uri;
  currentEtag = body.etag || null;
  currentFormat = body.format || "text";
  currentEditable = !!body.editable;
  loadedContent = body.content || "";

  uriInput.value = currentUri;
  editor.value = loadedContent;
  setDocumentIdentity(currentUri);
  updateEditorControls();
  setDocumentMode("view");
  await renderViewer(loadedContent, currentFormat);
  setSelection(currentUri, false);
  setStatus("saved", `Loaded: ${shortName(currentUri)}`);
}

function enterEditMode() {
  if (!currentUri) {
    setStatus("error", "Load a document first.");
    return;
  }
  if (!currentEditable) {
    setStatus(
      "locked",
      `Read-only document: ${currentFormat} format cannot be edited here.`,
    );
    return;
  }
  setDocumentMode("edit");
  setStatus("idle", "Edit mode");
}

async function saveDocument() {
  if (!currentUri) {
    setStatus("error", "Load a document first.");
    return;
  }
  if (!currentEditable) {
    setStatus("locked", `Read-only: ${currentFormat} is not editable.`);
    return;
  }

  setStatus("saving", "Saving and reindexing...");
  const resp = await fetch("/api/document/save", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      uri: currentUri,
      content: editor.value,
      expected_etag: currentEtag,
    }),
  });

  const body = await resp.json();
  if (!resp.ok) {
    const err = parseError(body);
    if (resp.status === 409) {
      setStatus("conflict", "Conflict: stale editor state. Reload and retry.");
      return;
    }
    if (resp.status === 423) {
      setStatus("locked", "Locked: another save is in progress.");
      return;
    }
    setStatus("error", `${err.code}: ${err.message}`);
    return;
  }

  currentEtag = body.etag || null;
  loadedContent = editor.value;
  setDocumentMode("view");
  await renderViewer(loadedContent, currentFormat);
  setStatus("saved", `Saved: ${shortName(currentUri)}`);
  await loadTree();
}

async function cancelEdit() {
  if (!isEditing) {
    return;
  }
  editor.value = loadedContent;
  setDocumentMode("view");
  await renderViewer(loadedContent, currentFormat);
  setStatus("idle", "Edit canceled");
}

async function mkdirSelected() {
  const base = selectedUri ? (selectedIsDir ? selectedUri : parentUri(selectedUri)) : treeRootInput.value.trim();
  if (!base) {
    setStatus("error", "Select a directory or set tree root first.");
    return;
  }

  const name = window.prompt("New folder name:");
  if (name == null) return;
  const uri = joinUri(base, name);
  if (!uri) {
    setStatus("error", "Folder name is required.");
    return;
  }

  setStatus("saving", `Creating: ${uri}`);
  const resp = await fetch("/api/fs/mkdir", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ uri }),
  });
  const body = await resp.json();
  if (!resp.ok) {
    const err = parseError(body);
    setStatus("error", `${err.code}: ${err.message}`);
    return;
  }

  setSelection(uri, true);
  setStatus("saved", `Created: ${shortName(uri)}`);
  await loadTree();
}

async function moveSelected() {
  if (!selectedUri) {
    setStatus("error", "Select a file or directory first.");
    return;
  }

  const target = window.prompt("Move to URI:", selectedUri);
  if (target == null) return;
  const toUri = target.trim();
  if (!toUri || toUri === selectedUri) {
    setStatus("error", "Target URI must be different.");
    return;
  }

  setStatus("saving", `Moving to ${toUri}`);
  const fromUri = selectedUri;
  const resp = await fetch("/api/fs/move", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ from_uri: fromUri, to_uri: toUri }),
  });
  const body = await resp.json();
  if (!resp.ok) {
    const err = parseError(body);
    setStatus("error", `${err.code}: ${err.message}`);
    return;
  }

  if (currentUri === fromUri) {
    currentUri = toUri;
    uriInput.value = toUri;
    setDocumentIdentity(toUri);
  }
  setSelection(toUri, selectedIsDir);
  setStatus("saved", `Moved: ${shortName(toUri)}`);
  await loadTree();
}

async function deleteSelected() {
  if (!selectedUri) {
    setStatus("error", "Select a file or directory first.");
    return;
  }

  const label = selectedIsDir ? "directory" : "file";
  const ok = window.confirm(`Delete ${label}?\n${selectedUri}`);
  if (!ok) return;

  const target = selectedUri;
  setStatus("saving", `Deleting: ${shortName(target)}`);
  const resp = await fetch("/api/fs/delete", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      uri: target,
      recursive: !!selectedIsDir,
    }),
  });
  const body = await resp.json();
  if (!resp.ok) {
    const err = parseError(body);
    setStatus("error", `${err.code}: ${err.message}`);
    return;
  }

  if (currentUri === target) {
    currentUri = null;
    currentEtag = null;
    currentFormat = "text";
    currentEditable = false;
    loadedContent = "";
    uriInput.value = "";
    editor.value = "";
    setDocumentIdentity(null);
    updateEditorControls();
    setDocumentMode("view");
    await renderViewer("", "text");
  }

  setSelection(null, false);
  setStatus("saved", `Deleted: ${shortName(target)}`);
  await loadTree();
}

loadBtn.addEventListener("click", () => {
  loadDocument().catch(() => setStatus("error", "Load request failed."));
});

reloadBtn.addEventListener("click", () => {
  loadDocument().catch(() => setStatus("error", "Reload request failed."));
});

docEditBtn.addEventListener("click", enterEditMode);
docSaveBtn.addEventListener("click", () => {
  saveDocument().catch(() => setStatus("error", "Save request failed."));
});
docCancelBtn.addEventListener("click", () => {
  cancelEdit().catch(() => setStatus("error", "Cancel failed."));
});

treeLoadBtn.addEventListener("click", () => {
  loadTree().catch(() => setStatus("error", "Tree load failed."));
});

treeRootInput.addEventListener("input", () => {
  updateRootPresetState(treeRootInput.value);
});

for (const button of rootPresetButtons) {
  button.addEventListener("click", () => {
    const root = button.dataset.root || "";
    treeRootInput.value = root;
    updateRootPresetState(root);
    loadTree().catch(() => setStatus("error", "Tree load failed."));
  });
}

showGeneratedInput.addEventListener("change", () => {
  showGenerated = !!showGeneratedInput.checked;
  loadTree().catch(() => setStatus("error", "Tree load failed."));
});

mkdirBtn.addEventListener("click", () => {
  mkdirSelected().catch(() => setStatus("error", "Create directory failed."));
});

moveBtn.addEventListener("click", () => {
  moveSelected().catch(() => setStatus("error", "Move failed."));
});

deleteBtn.addEventListener("click", () => {
  deleteSelected().catch(() => setStatus("error", "Delete failed."));
});

editor.addEventListener("input", () => {
  if (isEditing) {
    setStatus("idle", "Editing...");
  }
});

window.addEventListener("keydown", (event) => {
  const key = event.key.toLowerCase();
  if ((event.ctrlKey || event.metaKey) && key === "s" && isEditing) {
    event.preventDefault();
    saveDocument().catch(() => setStatus("error", "Save request failed."));
  }
  if ((event.ctrlKey || event.metaKey) && key === "o") {
    event.preventDefault();
    uriInput.focus();
    uriInput.select();
  }
});

const params = new URLSearchParams(window.location.search);
const uriParam = params.get("uri");
if (uriParam) {
  uriInput.value = uriParam;
  const parent = parentUri(uriParam);
  if (parent) {
    treeRootInput.value = parent;
  }
}

updateRootPresetState(treeRootInput.value);
setDocumentIdentity(null);
setDocumentMode("view");
updateEditorControls();
renderViewer("", "text").catch(() => setStatus("error", "Initial viewer render failed."));
loadTree().catch(() => setStatus("error", "Initial tree load failed."));
if (uriParam) {
  loadDocument().catch(() => setStatus("error", "Initial load failed."));
}
