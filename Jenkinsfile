pipeline {
    agent any

    stages {
        stage('Build') {
            steps {
                sh 'gmake'
            }
        }
        stage('Test') {
            steps {
                sh 'gmake test'
            }
        }
        stage('Release') {
            when { tag "v*" }
            steps {
                archiveArtifacts artifacts: 'artifacts/**', fingerprint: true
            }
        }
    }
}