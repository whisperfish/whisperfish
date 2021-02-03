import QtQuick 2.2
import Sailfish.Silica 1.0
import org.nemomobile.dbus 2.0

Page {
    id: captchaPage

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

    DBusAdaptor {
        service: "be.rubdos.whisperfish.captcha"
        path: "/be/rubdos/whisperfish"
        iface: "be.rubdos.whisperfish.captcha"

        function handleToken(tokenUrl) {
            console.log("Got a token URL:", tokenUrl);
            titleField.text = tokenUrl;
        }
    }
}
