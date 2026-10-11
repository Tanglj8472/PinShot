import sys
from PySide6.QtWidgets import QApplication, QWidget, QPushButton, QVBoxLayout, QLineEdit
app=QApplication(sys.argv); w=QWidget(); w.setWindowTitle("A11yProbe"); l=QVBoxLayout(w)
b=QPushButton("ProbeButton"); b.setAccessibleName("ProbeButton"); l.addWidget(b); l.addWidget(QLineEdit("x"))
w.setGeometry(1300,700,260,140); w.show(); app.exec()
