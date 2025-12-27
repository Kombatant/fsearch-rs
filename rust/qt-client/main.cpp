#include <QApplication>
#include <QListWidget>
#include <QVBoxLayout>
#include <QLineEdit>
#include <QPushButton>
#include <QTimer>
#include <QString>
#include <QLabel>
#include <QDir>
#include <QSettings>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QJsonValue>
#include <QPointer>
#include <algorithm>

#include <iostream>
#include <vector>
#include <atomic>

#include "../fsearch-core/include/fsearch_ffi.h"
#include "preferences.h"
#include "filter_editor.h"

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

static std::atomic<bool> g_shutting_down_main{false};

struct SearchContext {
    uint64_t handle; // 0 for index listing
    QListWidget *list;
    int maxResults;
    std::atomic<int> count;
    bool caseSensitive;
    bool useRegex;
    SearchContext(QListWidget *l = nullptr, int maxr = 1000, bool cs = false, bool ur = false)
        : handle(0), list(l), maxResults(maxr), count(0), caseSensitive(cs), useRegex(ur) {}
};

// Custom event and receiver for safe event-based delivery
class AddResultEvent : public QEvent {
public:
    static int eventType() {
        static int t = QEvent::registerEventType();
        return t;
    }
    QString name;
    QString path;
    QString highlightsJson;
    AddResultEvent(const QString &n, const QString &p, const QString &h)
        : QEvent((QEvent::Type)eventType()), name(n), path(p), highlightsJson(h) {}
};

class EventReceiver : public QObject {
    Q_OBJECT
    QListWidget *m_list;
public:
    explicit EventReceiver(QListWidget *list) : QObject(list), m_list(list) {}
    bool event(QEvent *e) override {
        if (e->type() == AddResultEvent::eventType()) {
            AddResultEvent *ar = static_cast<AddResultEvent*>(e);
            QString nameHtml = ar->name.toHtmlEscaped();
            QString pathHtml = ar->path.toHtmlEscaped();
            if (!ar->highlightsJson.isEmpty()) {
                QJsonParseError err;
                QJsonDocument doc = QJsonDocument::fromJson(ar->highlightsJson.toUtf8(), &err);
                if (err.error == QJsonParseError::NoError) {
                    if (doc.isObject()) {
                        QJsonObject obj = doc.object();
                        if (obj.contains("name") && obj.value("name").isArray()) nameHtml = applyRangesToHtml(ar->name, obj.value("name").toArray());
                        if (obj.contains("path") && obj.value("path").isArray()) pathHtml = applyRangesToHtml(ar->path, obj.value("path").toArray());
                    } else if (doc.isArray()) {
                        for (const QJsonValue &v : doc.array()) {
                            if (!v.isObject()) continue;
                            QJsonObject o = v.toObject();
                            QString field = o.value("field").toString();
                            QJsonArray ranges = o.value("ranges").toArray();
                            if (field == "name") nameHtml = applyRangesToHtml(ar->name, ranges);
                            else if (field == "path") pathHtml = applyRangesToHtml(ar->path, ranges);
                        }
                    }
                } else {
                    pathHtml = pathHtml + "<br><small>" + ar->highlightsJson.toHtmlEscaped() + "</small>";
                }
            }
            if (m_list) {
                QListWidgetItem *item = new QListWidgetItem(m_list);
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
                m_list->addItem(item);
                m_list->setItemWidget(item, itemWidget);
            }
            return true;
        }
        return QObject::event(e);
    }
};

extern "C" void result_cb(uint64_t id, const char *name, const char *path, uint64_t size, uint64_t mtime, const char *highlights, void *userdata) {
    if (g_shutting_down_main.load(std::memory_order_acquire)) return;
    SearchContext *ctx = static_cast<SearchContext *>(userdata);
    if (!ctx || !ctx->list) return;
    QString nameStr = QString::fromUtf8(name ? name : "");
    QString pathStr = QString::fromUtf8(path ? path : "");
    QString highlightsJson = QString::fromUtf8(highlights ? highlights : "");

    // Check result limit
    int prev = ctx->count.fetch_add(1, std::memory_order_acq_rel);
    bool reached = false;
    if (ctx->maxResults > 0 && (prev + 1) >= ctx->maxResults) reached = true;

    // Post AddResultEvent to the EventReceiver attached to the list
    EventReceiver *er = ctx->list->findChild<EventReceiver*>();
    if (!er) {
        er = new EventReceiver(ctx->list);
        er->setObjectName("fsearch_event_receiver");
    }
    AddResultEvent *ev = new AddResultEvent(nameStr, pathStr, highlightsJson);
    QCoreApplication::postEvent(er, ev);

    if (reached && ctx->handle != 0) {
        // Cancel the search and notify
        fsearch_cancel_search_c(ctx->handle);
        AddResultEvent *note = new AddResultEvent("", "", QString::fromUtf8("{\"field\":null,\"ranges\":[]}"));
        QCoreApplication::postEvent(er, note);
        // free context
        delete ctx;
    }
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

    QPushButton *prefsBtn = new QPushButton("Preferences", &w);
    layout->addWidget(prefsBtn);

    QPushButton *filtersBtn = new QPushButton("Filters", &w);
    layout->addWidget(filtersBtn);

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
            // use a temporary SearchContext for index listing (no handle)
            SearchContext *ctx = new SearchContext(resultsList, QSettings("fsearch","qt-client").value("maxResults", 1000).toInt(),
                                                   QSettings("fsearch","qt-client").value("caseSensitive", false).toBool(),
                                                   QSettings("fsearch","qt-client").value("useRegex", false).toBool());
            fsearch_index_list_entries_c(idx, result_cb, ctx);
            // keep idx allocated; in real app, manage lifetime
        }
    });

    uint64_t current_handle = 0;

    QObject::connect(searchBtn, &QPushButton::clicked, [&]() {
        resultsList->clear();

        // load preferences
        QSettings s("fsearch","qt-client");
        int maxResults = s.value("maxResults", 1000).toInt();
        bool caseSensitive = s.value("caseSensitive", false).toBool();
        bool useRegexDefault = s.value("useRegex", false).toBool();

        QString q = queryInput->text();
        QByteArray qb = q.toUtf8();
        // create options from preferences
        fsearch_search_options_t opts;
        opts.max_results = static_cast<uint32_t>(maxResults);
        opts.case_sensitive = caseSensitive ? 1 : 0;
        opts.use_regex = useRegexDefault ? 1 : 0;
        // create context and start search with explicit options
        SearchContext *ctx = new SearchContext(resultsList, maxResults, caseSensitive, useRegexDefault);
        uint64_t handle = fsearch_start_search_with_opts_c(qb.constData(), &opts, result_cb, ctx);
        ctx->handle = handle;
        current_handle = handle;
    });

    QObject::connect(prefsBtn, &QPushButton::clicked, [&]() {
        PreferencesDialog dlg(&w);
        if (dlg.exec() == QDialog::Accepted) {
            QString info = QString("Preferences saved: max=%1 case=%2 regex=%3")
                .arg(dlg.maxResults())
                .arg(dlg.caseSensitive() ? "yes" : "no")
                .arg(dlg.useRegex() ? "yes" : "no");
            resultsList->addItem(info);
        }
    });

    QObject::connect(filtersBtn, &QPushButton::clicked, [&]() {
        FilterEditorDialog dlg(&w);
        if (dlg.exec() == QDialog::Accepted) {
            resultsList->addItem("Filters updated");
        }
    });

    w.setWindowTitle("FSearch Qt6 Test Client");
    w.resize(800, 600);
    w.show();

    QObject::connect(&app, &QCoreApplication::aboutToQuit, []() {
        g_shutting_down_main.store(true);
        fprintf(stderr, "main: aboutToQuit - g_shutting_down_main set\n");
    });

    return app.exec();
}

#include "main.moc"

