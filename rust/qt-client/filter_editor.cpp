#include "filter_editor.h"
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QPushButton>
#include <QListWidget>
#include <QLineEdit>
#include <QSettings>
#include <QLabel>

FilterEditorDialog::FilterEditorDialog(QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle("Filter Editor");
    QVBoxLayout *v = new QVBoxLayout(this);
    m_list = new QListWidget(this);
    m_input = new QLineEdit(this);
    m_input->setPlaceholderText("Enter filter expression (e.g. path:src)");

    QHBoxLayout *addRow = new QHBoxLayout();
    addRow->addWidget(m_input);
    QPushButton *addBtn = new QPushButton("Add", this);
    addRow->addWidget(addBtn);

    v->addWidget(new QLabel("Saved filters:" , this));
    v->addWidget(m_list);
    v->addLayout(addRow);

    QHBoxLayout *bottom = new QHBoxLayout();
    QPushButton *removeBtn = new QPushButton("Remove Selected", this);
    bottom->addWidget(removeBtn);
    bottom->addStretch();
    QPushButton *ok = new QPushButton("OK", this);
    QPushButton *cancel = new QPushButton("Cancel", this);
    bottom->addWidget(ok);
    bottom->addWidget(cancel);
    v->addLayout(bottom);

    // load saved filters
    QSettings s("fsearch","qt-client");
    QStringList list = s.value("filters", QStringList()).toStringList();
    for (const QString &f : list) m_list->addItem(f);

    connect(addBtn, &QPushButton::clicked, this, &FilterEditorDialog::addFilter);
    connect(removeBtn, &QPushButton::clicked, this, &FilterEditorDialog::removeSelected);
    connect(ok, &QPushButton::clicked, this, [this]() { save(); accept(); });
    connect(cancel, &QPushButton::clicked, this, &FilterEditorDialog::reject);
}

bool FilterEditorDialog::save() {
    QStringList out;
    for (int i = 0; i < m_list->count(); ++i) out << m_list->item(i)->text();
    QSettings s("fsearch","qt-client");
    s.setValue("filters", out);
    s.sync();
    return true;
}

void FilterEditorDialog::addFilter() {
    QString t = m_input->text().trimmed();
    if (t.isEmpty()) return;
    m_list->addItem(t);
    m_input->clear();
}

void FilterEditorDialog::removeSelected() {
    auto items = m_list->selectedItems();
    for (QListWidgetItem *it : items) {
        delete m_list->takeItem(m_list->row(it));
    }
}
