#include <QGuiApplication>
#include <QCoreApplication>
#include <QWindow>
#include <QObject>
#include <QDebug>
#include <QTimer>
#include <QMetaObject>
#include <atomic>
#include <thread>
#include <vector>
#include <chrono>

struct Receiver : public QObject {
    std::atomic<int> count{0};
    Receiver(QObject *parent = nullptr) : QObject(parent) {}
};

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);

    // Default options
    std::string target = "receiver"; // receiver | app | window
    int threads = 1;
    int events = 20000;
    int quit_ms = 200;
    bool delete_before = false;

    for (int i = 1; i < argc; ++i) {
        std::string s(argv[i]);
        if (s.rfind("--target=", 0) == 0) target = s.substr(9);
        if (s.rfind("--threads=", 0) == 0) threads = std::stoi(s.substr(10));
        if (s.rfind("--events=", 0) == 0) events = std::stoi(s.substr(9));
        if (s.rfind("--quit-ms=", 0) == 0) quit_ms = std::stoi(s.substr(10));
        if (s == "--delete-before") delete_before = true;
    }

    QObject *target_obj = nullptr;
    Receiver *receiver = nullptr;
    QWindow *window = nullptr;

    if (target == "app") {
        target_obj = QCoreApplication::instance();
    } else if (target == "window") {
        window = new QWindow();
        target_obj = window;
    } else {
        receiver = new Receiver();
        target_obj = receiver;
    }

    // Producer threads post queued lambdas to the chosen target
    std::vector<std::thread> producers;
    int per_thread = events / std::max(1, threads);
    for (int t = 0; t < threads; ++t) {
        producers.emplace_back([t, per_thread, target_obj]() {
            for (int i = 0; i < per_thread; ++i) {
                QMetaObject::invokeMethod(target_obj, [target_obj]() {
                    // If target is Receiver, increment; otherwise do nothing
                    Receiver *r = dynamic_cast<Receiver*>(target_obj);
                    if (r) r->count.fetch_add(1, std::memory_order_relaxed);
                }, Qt::QueuedConnection);
            }
        });
    }

    // Optionally delete the receiver (on the main thread) while events are queued
    if (delete_before && receiver) {
        std::thread deleter([receiver, quit_ms]() {
            // Sleep a short time to let producers post
            std::this_thread::sleep_for(std::chrono::milliseconds(std::max(1, quit_ms/10)));
            // Delete on the main thread via queued invocation
            QMetaObject::invokeMethod(QCoreApplication::instance(), [receiver]() {
                delete receiver;
            }, Qt::BlockingQueuedConnection);
        });
        deleter.detach();
    }

    // Quit after a short delay
    QTimer::singleShot(quit_ms, &app, [&app]() { app.quit(); });

    int res = app.exec();

    for (auto &th : producers) if (th.joinable()) th.join();

    if (receiver) qDebug() << "Handled count:" << receiver->count.load();
    else qDebug() << "Handled (target) done";

    if (window) delete window;
    return res;
}
