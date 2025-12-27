#pragma once

#include <QDialog>

class QListWidget;
class QLineEdit;

class FilterEditorDialog : public QDialog {
    Q_OBJECT
public:
    explicit FilterEditorDialog(QWidget *parent = nullptr);
    int maxResults() const;
    bool save();

private slots:
    void addFilter();
    void removeSelected();

private:
    QListWidget *m_list;
    QLineEdit *m_input;
};
