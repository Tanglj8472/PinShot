"""Local OCR and Edge translation, following SuperTranslate's user flow."""
import json
import threading
import sys
import urllib.parse
import urllib.request
from pathlib import Path
from PySide6.QtCore import QObject, Signal, Qt, QPoint, QStandardPaths
from PySide6.QtGui import QCursor, QGuiApplication, QImage, QPixmap
from PySide6.QtWidgets import QWidget, QVBoxLayout, QHBoxLayout, QLabel, QTextEdit, QPushButton, QComboBox

_ocr_lock = threading.Lock()
_ocr_engine = None


def recognize(image):
    global _ocr_engine
    import numpy as np
    from rapidocr import RapidOCR
    from rapidocr.utils.typings import OCRVersion, ModelType
    with _ocr_lock:
        if _ocr_engine is None:
            from .settings import data_dir
            model_dir = data_dir() / 'ocr-models'
            model_dir.mkdir(parents=True, exist_ok=True)
            params = {
                'Global.model_root_dir': str(model_dir),
                'Det.ocr_version': OCRVersion('PP-OCRv5'), 'Rec.ocr_version': OCRVersion('PP-OCRv5'),
                'Cls.ocr_version': OCRVersion('PP-OCRv5'),
                'Det.model_type': ModelType('mobile'), 'Rec.model_type': ModelType('mobile'),
                'Det.limit_type': 'max', 'Det.limit_side_len': 960,
                'EngineConfig.onnxruntime.intra_op_num_threads': 2,
                'EngineConfig.onnxruntime.inter_op_num_threads': 2,
            }
            bundled = Path(getattr(sys, '_MEIPASS', Path(__file__).resolve().parents[1])) / 'assets' / 'ocr-models'
            for kind, name in [('Det', 'ch_PP-OCRv5_det_mobile'), ('Rec', 'ch_PP-OCRv5_rec_mobile'),
                               ('Cls', 'ch_PP-LCNet_x0_25_textline_ori_cls_mobile')]:
                model = bundled / (name + '.onnx')
                if model.exists(): params[kind + '.model_path'] = str(model)
            _ocr_engine = RapidOCR(params=params)
        rgb = image.convertToFormat(QImage.Format_RGB888)
        pixels = np.frombuffer(rgb.constBits(), dtype=np.uint8).reshape(rgb.height(), rgb.bytesPerLine())
        pixels = pixels[:, :rgb.width() * 3].reshape(rgb.height(), rgb.width(), 3)
        output = _ocr_engine(pixels[:, :, ::-1].copy())
        return '\n'.join(output.txts or [])


def read_translation(body):
    data = json.loads(body)
    parts = [t['text'] for item in data for t in item.get('translations', []) if isinstance(t.get('text'), str)]
    if not parts: raise ValueError('No translation returned')
    return '\n'.join(parts)


def translate(text, target='zh-Hans'):
    if not text.strip(): raise ValueError('No text to translate')
    query = urllib.parse.urlencode({'from': '', 'to': target, 'isEnterpriseClient': 'false'})
    req = urllib.request.Request('https://edge.microsoft.com/translate/translatetext?' + query,
        data=json.dumps([text], ensure_ascii=False).encode('utf-8'),
        headers={'Content-Type': 'application/json', 'Accept': 'application/json',
                 'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/128.0.0.0 Safari/537.36 Edg/128.0.0.0'})
    with urllib.request.urlopen(req, timeout=8) as response:
        return read_translation(response.read().decode('utf-8'))


class ResultWindow(QWidget):
    def __init__(self):
        super().__init__(None, Qt.Window | Qt.WindowStaysOnTopHint)
        from .i18n import tr
        self.setWindowTitle(tr('PinShot OCR'))
        self.resize(480, 460)
        self.closed = False; self.request = 0; self.busy = False
        layout = QVBoxLayout(self)
        layout.addWidget(QLabel('Original (editable)'))
        self.original = QTextEdit(); self.original.setAcceptRichText(False); layout.addWidget(self.original)
        row = QHBoxLayout()
        self.language = QComboBox()
        self.language.setObjectName('target_language')
        self.language.setProperty('i18n', False)
        for name, code in [('简体中文', 'zh-Hans'), ('English', 'en'), ('日本語', 'ja'), ('繁體中文', 'zh-Hant')]:
            self.language.addItem(name, code)
        row.addWidget(self.language)
        self.translate_button = QPushButton('Translate'); row.addWidget(self.translate_button)
        self.copy_original = QPushButton('Copy original'); row.addWidget(self.copy_original)
        layout.addLayout(row)
        layout.addWidget(QLabel('Translation'))
        self.translated = QTextEdit(); self.translated.setReadOnly(True); layout.addWidget(self.translated)
        self.copy_translated = QPushButton('Copy translation'); layout.addWidget(self.copy_translated)
        self.status = QLabel('OCR runs on this device. Translation sends text online.')
        self.status.setWordWrap(True); layout.addWidget(self.status)
        self.copy_original.clicked.connect(lambda: QGuiApplication.clipboard().setText(self.original.toPlainText()))
        self.copy_translated.clicked.connect(lambda: QGuiApplication.clipboard().setText(self.translated.toPlainText()))
        self.copy_original.setEnabled(False)
        self.original.textChanged.connect(lambda: self.copy_original.setEnabled(bool(self.original.toPlainText().strip())))
        self.copy_translated.setEnabled(False)
        scr = QGuiApplication.screenAt(QCursor.pos()) or QGuiApplication.primaryScreen()
        area = scr.availableGeometry()
        self.move(QPoint(max(area.left(), min(QCursor.pos().x(), area.right()-self.width())),
                         max(area.top(), min(QCursor.pos().y(), area.bottom()-self.height()))))

    def showEvent(self, event):
        super().showEvent(event)
        self.raise_above_pins()
        from PySide6.QtCore import QTimer
        for ms in (0, 60, 250):   # WMs restack asynchronously after map
            QTimer.singleShot(ms, lambda: None if self.closed else self.raise_above_pins())

    def raise_above_pins(self):
        """Pins and this window share the always-on-top layer; push pins to the bottom of that
        layer (XLowerWindow) and raise this one (XRaiseWindow + activate)."""
        try:
            from .pin_window import ALL_PINS
            for p in list(ALL_PINS):
                if p.isVisible(): p.lower()
        except Exception:
            pass
        self.raise_(); self.activateWindow()

    def closeEvent(self, event):
        self.closed = True; self.request += 1
        super().closeEvent(event)


class TranslationController(QObject):
    completed = Signal(object, int, str, str, str, bool)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.windows = []
        self.completed.connect(self._completed)

    def new_window(self):
        # Closed windows are kept alive until their pending worker returns.
        self.windows = [w for w in self.windows if not w.closed or getattr(w, 'busy', False)]
        window = ResultWindow(); self.windows.append(window)
        window.translate_button.clicked.connect(lambda: self.start_translation(window))
        window.show(); window.raise_(); window.activateWindow()
        from PySide6.QtCore import QTimer
        return window

    def image(self, image, auto_translate=False):
        window = self.new_window()
        preview = QLabel()
        preview.setPixmap(QPixmap.fromImage(image).scaled(440, 120, Qt.KeepAspectRatio, Qt.SmoothTransformation))
        preview.setAlignment(Qt.AlignCenter)
        window.layout().insertWidget(0, preview)
        window.translate_button.setEnabled(False)
        window.original.setReadOnly(True)
        self._run(window, 'ocr', lambda: recognize(image), auto_translate)
        return window

    def text(self, text):
        window = self.new_window(); window.original.setPlainText(text)
        if text.strip(): self.start_translation(window)
        else: window.status.setText('No selected text. Select text, then press the hotkey.')
        return window

    def start_translation(self, window):
        text = window.original.toPlainText().strip()
        if not text: window.status.setText('No text to translate'); return
        target = window.language.currentData()
        window.translated.clear(); window.copy_translated.setEnabled(False)
        self._run(window, 'translate', lambda: translate(text, target))

    def _run(self, window, kind, function, auto_translate=False):
        window.request += 1; request = window.request; window.busy = True
        window.translate_button.setEnabled(False)
        window.status.setText('Recognizing…' if kind == 'ocr' else 'Translating…')
        def work():
            result = error = ''
            try: result = function()
            except Exception as exc: error = str(exc)
            try: self.completed.emit(window, request, kind, result, error, auto_translate)
            except RuntimeError: pass  # application was closed while worker was running
        threading.Thread(target=work, daemon=True).start()

    def _completed(self, window, request, kind, result, error, auto_translate):
        if not window.closed and window.request != request: return
        window.busy = False
        if window.closed: return
        window.translate_button.setEnabled(True)
        if kind == 'ocr': window.original.setReadOnly(False)
        if error:
            window.status.setText(('OCR failed: ' if kind == 'ocr' else 'Translation failed: ') + error)
            return
        if kind == 'ocr':
            window.original.setPlainText(result)
            window.status.setText('OCR done. Edit, copy, or translate.' if result.strip() else 'No text found. Select a clearer area.')
            if auto_translate and result.strip(): self.start_translation(window)
        else:
            window.translated.setPlainText(result); window.copy_translated.setEnabled(True)
            window.status.setText('Translation done')
