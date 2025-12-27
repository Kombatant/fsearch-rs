#pragma once

#include <QDialog>

class QCheckBox;
class QSpinBox;
class QPushButton;

class PreferencesDialog : public QDialog {
    Q_OBJECT
public:
    explicit PreferencesDialog(QWidget *parent = nullptr);
    int maxResults() const;
    bool caseSensitive() const;
    bool useRegex() const;

private:
    QSpinBox *m_maxResults;
    QCheckBox *m_caseSensitive;
    QCheckBox *m_useRegex;
    QPushButton *m_ok;
    QPushButton *m_cancel;
};
