"""Download the official RapidOCR PP-OCRv5 mobile models for offline packaging."""
import concurrent.futures
import hashlib
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parents[1] / 'assets' / 'ocr-models'
MODELS = [
    ('det', 'ch_PP-OCRv5_det_mobile', '4d97c44a20d30a81aad087d6a396b08f786c4635742afc391f6621f5c6ae78ae'),
    ('rec', 'ch_PP-OCRv5_rec_mobile', '5825fc7ebf84ae7a412be049820b4d86d77620f204a041697b0494669b1742c5'),
    ('cls', 'ch_PP-LCNet_x0_25_textline_ori_cls_mobile', '54379ae5174d026780215fc748a7f31910dee36818e63d49e17dc598ecc82df7'),
]


def download(model):
    kind, name, digest = model
    target = ROOT / (name + '.onnx')
    if target.exists() and hashlib.sha256(target.read_bytes()).hexdigest() == digest:
        return name + ' already verified'
    url = f'https://www.modelscope.cn/models/RapidAI/RapidOCR/resolve/v3.10.0/onnx/PP-OCRv5/{kind}/{name}.onnx'
    with urllib.request.urlopen(url, timeout=30) as response:
        data = response.read()
    if hashlib.sha256(data).hexdigest() != digest: raise ValueError(name + ': SHA256 mismatch')
    target.write_bytes(data)
    return name + ' verified'


if __name__ == '__main__':
    ROOT.mkdir(parents=True, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as executor:
        for result in executor.map(download, MODELS): print(result, flush=True)
