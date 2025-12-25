#include <QApplication>
#include <QListWidget>
#include <QVBoxLayout>
#include <QLineEdit>
#include <QPushButton>
#include <QTimer>
#include <QString>
#include <QLabel>
#include <QDir>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QJsonValue>
#include <algorithm>

#include <iostream>
#include <vector>

#include "../fsearch-core/include/fsearch_ffi.h"

static QString applyRangesToHtml(const QString &text, const QJsonArray &rangesArray) {
    // rangesArray: array of [start,end] integer arrays (character indices)
    QVector<QPair<int,int>> ranges;
    for (const QJsonValue &rv : rangesArray) {
        if (!rv.isArray()) continue;
        QJsonArray a = rv.toArray();
        if (a.size() < 2) continue;
        int s = a.at(0).toInt(-1);
        int e = a.at(1).toInt(-1);
        if (s < 0 || e <= s) continue;
        ranges.append(qMakePair(s, e));
    }
    if (ranges.isEmpty()) return text.toHtmlEscaped();
    // merge & sort
    std::sort(ranges.begin(), ranges.end(), [](const QPair<int,int>&x,const QPair<int,int>&y){ return x.first < y.first; });
    QVector<QPair<int,int>> merged;
    for (auto &r : ranges) {
        if (merged.isEmpty() || r.first > merged.last().second) merged.append(r);
        else merged.last().second = std::max(merged.last().second, r.second);
    }
    QString out;
    int pos = 0;
    for (auto &r : merged) {
        int s = r.first;
        int e = r.second;
        if (s > text.size()) break;
        if (s > pos) out += text.mid(pos, s-pos).toHtmlEscaped();
        int len = qMin(e, text.size()) - s;
        if (len > 0) out += "<b>" + text.mid(s, len).toHtmlEscaped() + "</b>";
        pos = qMin(e, text.size());
    }
    if (pos < text.size()) out += text.mid(pos).toHtmlEscaped();
    return out;
}

extern "C" void result_cb(uint64_t id, const char *name, const char *path, uint64_t size, uint64_t mtime, const char *highlights, void *userdata) {
    // Called from Rust search threads. Post to Qt main thread via queued invocation.
    QListWidget *list = static_cast<QListWidget *>(userdata);
    if (!list) return;
    QString nameStr = QString::fromUtf8(name ? name : "");
    QString pathStr = QString::fromUtf8(path ? path : "");
    QString highlightsJson = QString::fromUtf8(highlights ? highlights : "");

    QMetaObject::invokeMethod(QApplication::instance(), [list, nameStr, pathStr, highlightsJson]() {
        QString nameHtml = nameStr.toHtmlEscaped();
        QString pathHtml = pathStr.toHtmlEscaped();
        if (!highlightsJson.isEmpty()) {
            QJsonParseError err;
            QJsonDocument doc = QJsonDocument::fromJson(highlightsJson.toUtf8(), &err);
            if (err.error == QJsonParseError::NoError) {
                if (doc.isObject()) {
                    QJsonObject obj = doc.object();
                    if (obj.contains("name") && obj.value("name").isArray()) nameHtml = applyRangesToHtml(nameStr, obj.value("name").toArray());
                    if (obj.contains("path") && obj.value("path").isArray()) pathHtml = applyRangesToHtml(pathStr, obj.value("path").toArray());
                } else if (doc.isArray()) {
                    for (const QJsonValue &v : doc.array()) {
                        if (!v.isObject()) continue;
                        QJsonObject o = v.toObject();
                        QString field = o.value("field").toString();
                        QJsonArray ranges = o.value("ranges").toArray();
                        if (field == "name") nameHtml = applyRangesToHtml(nameStr, ranges);
                        else if (field == "path") pathHtml = applyRangesToHtml(pathStr, ranges);
                    }
                }
            } else {
                pathHtml = pathHtml + "<br><small>" + highlightsJson.toHtmlEscaped() + "</small>";
            }
        }

        QListWidgetItem *item = new QListWidgetItem(list);
        QWidget *itemWidget = new QWidget();
        QVBoxLayout *vlayout = new QVBoxLayout(itemWidget);
        vlayout->setContentsMargins(4,2,4,2);
        QLabel *nameLabel = new QLabel(itemWidget);
        nameLabel->setTextFormat(Qt::RichText);
        nameLabel->setText(nameHtml);
        QLabel *pathLabel = new QLabel(itemWidget);
        pathLabel->setTextFormat(Qt::RichText);
        pathLabel->setText("<small>" + pathHtml + "</small>");
        vlayout->addWidget(nameLabel);
        vlayout->addWidget(pathLabel);
        item->setSizeHint(itemWidget->sizeHint());
        list->addItem(item);
        list->setItemWidget(item, itemWidget);
    }, Qt::QueuedConnection);
}

int main(int argc, char **argv) {
    QApplication app(argc, argv);

    QWidget w;
    QVBoxLayout *layout = new QVBoxLayout(&w);

    QLineEdit *pathInput = new QLineEdit(&w);
    pathInput->setPlaceholderText("Enter path to index (comma-separated) or leave empty for current dir");
    layout->addWidget(new QLabel("Index paths (comma-separated):"));
    layout->addWidget(pathInput);

    QLineEdit *queryInput = new QLineEdit(&w);
    queryInput->setPlaceholderText("Enter query (prefix with re: for regex)");
    layout->addWidget(new QLabel("Query:"));
    layout->addWidget(queryInput);

    QPushButton *indexBtn = new QPushButton("Build Index", &w);
    layout->addWidget(indexBtn);

    QPushButton *searchBtn = new QPushButton("Start Search", &w);
    layout->addWidget(searchBtn);

    QListWidget *resultsList = new QListWidget(&w);
    layout->addWidget(resultsList);

    QObject::connect(indexBtn, &QPushButton::clicked, [&]() {
        QString paths = pathInput->text();
        std::vector<const char *> cpaths;
        if (paths.isEmpty()) {
            QByteArray cwd = QDir::currentPath().toUtf8();
            cpaths.push_back(strdup(cwd.constData()));
        } else {
            const QStringList parts = paths.split(',', Qt::SkipEmptyParts);
            for (const QString &p : parts) {
                QByteArray ba = p.trimmed().toUtf8();
                cpaths.push_back(strdup(ba.constData()));
            }
        }
        // build index
        void *idx = fsearch_index_build_from_paths_c(cpaths.data(), cpaths.size());
        // free duplicated C strings
        for (auto s : cpaths) free((void *)s);
        if (!idx) {
            resultsList->addItem("Index build failed");
        } else {
            resultsList->addItem("Index built — listing entries:");
            fprintf(stderr, "calling fsearch_index_list_entries_c with idx=%p resultsList=%p\n", idx, resultsList);
            fflush(stderr);
            fsearch_index_list_entries_c(idx, result_cb, resultsList);
            // keep idx allocated; in real app, manage lifetime
        }
    });

    uint64_t current_handle = 0;
    // No polling timer — we'll use event-driven delivery from Rust.

    QObject::connect(searchBtn, &QPushButton::clicked, [&]() {
        resultsList->clear();
        QString q = queryInput->text();
        QByteArray qb = q.toUtf8();
        current_handle = fsearch_start_search_with_cb_c(qb.constData(), result_cb, resultsList);
    });

    w.setWindowTitle("FSearch Qt6 Test Client");
    w.resize(800, 600);
    w.show();

    return app.exec();
}
