#include "preferences.h"
#include <QCheckBox>
#include <QSpinBox>
#include <QFormLayout>
#include <QDialogButtonBox>
#include <QSettings>

PreferencesDialog::PreferencesDialog(QWidget *parent)
    : QDialog(parent)
{
    setWindowTitle("Preferences");
    m_maxResults = new QSpinBox(this);
    m_maxResults->setRange(1, 100000);
    m_caseSensitive = new QCheckBox("Case sensitive", this);
    m_useRegex = new QCheckBox("Treat queries as regex by default", this);

    QFormLayout *fl = new QFormLayout(this);
    fl->addRow("Max results:", m_maxResults);
    fl->addRow("", m_caseSensitive);
    fl->addRow("", m_useRegex);

    QDialogButtonBox *bb = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, this);
    connect(bb, &QDialogButtonBox::accepted, this, &PreferencesDialog::accept);
    connect(bb, &QDialogButtonBox::rejected, this, &PreferencesDialog::reject);
    fl->addRow(bb);

    // load settings
    QSettings s("fsearch", "qt-client");
    m_maxResults->setValue(s.value("maxResults", 1000).toInt());
    m_caseSensitive->setChecked(s.value("caseSensitive", false).toBool());
    m_useRegex->setChecked(s.value("useRegex", false).toBool());

    connect(this, &QDialog::accepted, [this](){
        QSettings s("fsearch", "qt-client");
        s.setValue("maxResults", m_maxResults->value());
        s.setValue("caseSensitive", m_caseSensitive->isChecked());
        s.setValue("useRegex", m_useRegex->isChecked());
        s.sync();
    });
}

int PreferencesDialog::maxResults() const { return m_maxResults->value(); }
bool PreferencesDialog::caseSensitive() const { return m_caseSensitive->isChecked(); }
bool PreferencesDialog::useRegex() const { return m_useRegex->isChecked(); }
