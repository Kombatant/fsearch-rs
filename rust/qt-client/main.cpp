#include <QApplication>
#include <QListWidget>
#include <QVBoxLayout>
#include <QLineEdit>
#include <QPushButton>
#include <QTimer>
#include <QString>
#include <QLabel>
#include <QDir>

#include <iostream>
#include <vector>

#include "../fsearch-core/include/fsearch_ffi.h"

static void result_cb(uint64_t id, const char *name, const char *path, uint64_t size, uint64_t mtime, void *userdata) {
    QListWidget *list = static_cast<QListWidget *>(userdata);
    QString text = QString("%1 — %2").arg(QString::fromUtf8(name)).arg(QString::fromUtf8(path));
    list->addItem(text);
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
            fsearch_index_list_entries_c(idx, result_cb, resultsList);
            // keep idx allocated; in real app, manage lifetime
        }
    });

    uint64_t current_handle = 0;
    QTimer pollTimer;
    pollTimer.setInterval(200);

    QObject::connect(&pollTimer, &QTimer::timeout, [&]() {
        if (current_handle != 0) {
            fsearch_poll_results_c(current_handle, result_cb, resultsList);
        }
    });

    QObject::connect(searchBtn, &QPushButton::clicked, [&]() {
        resultsList->clear();
        QString q = queryInput->text();
        QByteArray qb = q.toUtf8();
        current_handle = fsearch_start_search_c(qb.constData());
        if (current_handle != 0) {
            pollTimer.start();
        }
    });

    w.setWindowTitle("FSearch Qt6 Test Client");
    w.resize(800, 600);
    w.show();

    return app.exec();
}
