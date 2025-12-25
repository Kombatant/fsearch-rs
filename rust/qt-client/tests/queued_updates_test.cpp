#include <QApplication>
#include <QListWidget>
#include <QThread>
#include <QMetaObject>
#include <QTimer>
#include <QWidget>
#include <QVBoxLayout>
#include <QLabel>
#include <cstdio>
#include <thread>
#include <chrono>

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    QWidget *w = new QWidget();
    QVBoxLayout *layout = new QVBoxLayout(w);
    layout->addWidget(new QLabel("Queued updates test"));
    QListWidget *list = new QListWidget(w);
    layout->addWidget(list);
    w->show();

    // Spawn background thread that posts many queued updates
    std::thread worker([list]() {
        for (int i = 0; i < 5000; ++i) {
            std::string s = std::string("item-") + std::to_string(i);
            // Post via invokeMethod on the QApplication object; construct QString on GUI thread
            QMetaObject::invokeMethod(QApplication::instance(), [s, list]() {
                new QListWidgetItem(QString::fromUtf8(s.c_str()), list);
            }, Qt::QueuedConnection);
            if ((i & 63) == 0) std::this_thread::sleep_for(std::chrono::milliseconds(1));
        }
    });

    // Wait for worker to enqueue many events
    worker.join();

    // Drain events for a moment
    for (int i = 0; i < 200; ++i) {
        QCoreApplication::processEvents();
        QThread::msleep(5);
    }

    // Explicitly delete main window and children then process events
    fprintf(stderr, "deleting main window\n");
    delete w;
    w = nullptr;
    QCoreApplication::processEvents();

    return 0;
}
