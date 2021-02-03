import QtQuick 2.2
import Sailfish.Silica 1.0
import Sailfish.WebView 1.0

WebViewPage {
    id: webviewpage

    allowedOrientations: Orientation.All

	Label {
		id: titleField
		anchors {
			left: parent.left
			right: parent.right
			top: parent.top
		}

		//: Title for captcha web view
		//% "Registration Captcha"
		text: qsTrId("whisperfish-registration-captcha-title")

	}

    WebViewFlickable {
		anchors {
			top: parent.top//titleField.bottom
			left: parent.left
			right: parent.right
			bottom: parent.bottom
		}

        WebView {
            anchors.fill: parent
            active: true
			url: "https://signalcaptchas.org/registration/generate.html"
        }
    }
}
