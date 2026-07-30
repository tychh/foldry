import { RangeSetBuilder } from "@codemirror/state";
import { linter, lintGutter, type Diagnostic } from "@codemirror/lint";
import {
  Decoration,
  EditorView,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";
import CodeMirror from "@uiw/react-codemirror";
import { useMemo } from "react";

import type { ParserDiagnostic } from "../../shared/contracts/generated";

type ProfileCodeEditorProps = {
  diagnostics: ParserDiagnostic[];
  value: string;
  onChange: (value: string) => void;
};

export function ProfileCodeEditor({
  diagnostics,
  value,
  onChange,
}: ProfileCodeEditorProps) {
  const diagnosticsExtension = useMemo(
    () =>
      linter((view) =>
        diagnostics.map((diagnostic) =>
          toCodeMirrorDiagnostic(view, diagnostic),
        ),
      ),
    [diagnostics],
  );

  return (
    <CodeMirror
      basicSetup={{
        bracketMatching: false,
        closeBrackets: false,
        autocompletion: false,
        foldGutter: false,
        highlightActiveLine: true,
        highlightActiveLineGutter: true,
        lineNumbers: true,
      }}
      className="foldry-profile-editor"
      extensions={[
        profileSyntax,
        diagnosticsExtension,
        lintGutter(),
        EditorView.lineWrapping,
      ]}
      height="100%"
      theme="none"
      value={value}
      onChange={onChange}
    />
  );
}

const profileSyntax = ViewPlugin.fromClass(
  class {
    decorations: ReturnType<typeof buildDecorations>;

    constructor(view: EditorView) {
      this.decorations = buildDecorations(view);
    }

    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) {
        this.decorations = buildDecorations(update.view);
      }
    }
  },
  {
    decorations: (plugin) => plugin.decorations,
  },
);

function buildDecorations(view: EditorView) {
  const builder = new RangeSetBuilder<Decoration>();
  for (const range of view.visibleRanges) {
    let position = range.from;
    while (position <= range.to) {
      const line = view.state.doc.lineAt(position);
      const value = line.text.trimStart();
      let className = "foldry-rule";
      if (value.startsWith("# @profile-")) {
        className = "foldry-metadata";
      } else if (value.startsWith("# @preset-")) {
        className = "foldry-preset-marker";
      } else if (value.startsWith("#") || value.length === 0) {
        className = "foldry-comment";
      } else if (value.startsWith("!")) {
        className = "foldry-negation";
      }
      builder.add(line.from, line.from, Decoration.line({ class: className }));
      position = line.to + 1;
      if (line.to === view.state.doc.length) {
        break;
      }
    }
  }
  return builder.finish();
}

function toCodeMirrorDiagnostic(
  view: EditorView,
  diagnostic: ParserDiagnostic,
): Diagnostic {
  const lineNumber = Math.min(
    Math.max(diagnostic.line ?? 1, 1),
    view.state.doc.lines,
  );
  const line = view.state.doc.line(lineNumber);
  const startColumn = Math.max((diagnostic.start_column ?? 1) - 1, 0);
  const endColumn = Math.max(
    (diagnostic.end_column ?? diagnostic.start_column ?? 1) - 1,
    startColumn + 1,
  );
  return {
    from: Math.min(line.from + startColumn, line.to),
    to: Math.min(line.from + endColumn, line.to),
    severity: diagnostic.severity,
    message: diagnostic.message,
  };
}
